use std::{io::BufRead, time::Duration};

use orfail::OrFail;

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("stats")
        .doc(concat!(
            "Calculate statistics from JSON objects outputted ",
            "by executing the command `call --add-metadata ...`\n",
            "\n",
            "Note that the output of `call` command does not include notifications,\n",
            "so the statistics do not take them into account."
        ))
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    if args.metadata().help_mode {
        return Ok(false);
    }

    run_stats()?;
    Ok(true)
}

fn run_stats() -> orfail::Result<()> {
    let stdin = std::io::stdin();
    let mut stats = Stats::default();

    let reader = std::io::BufReader::new(stdin.lock());
    for line in reader.lines() {
        let line = line.or_fail()?;
        let json = nojson::RawJson::parse(&line).or_fail()?;
        stats.handle_output(json.value()).or_fail()?;
    }
    stats.finalize();
    println!("{}", nojson::Json(&stats));
    Ok(())
}

#[derive(Debug, Default)]
struct Stats {
    duration: Duration,
    max_concurrency: usize,
    request_count: usize,
    response_ok_count: usize,
    response_error_count: usize,
    latency_min: f64,
    latency_p25: f64,
    latency_p50: f64,
    latency_p75: f64,
    latency_max: f64,
    latency_avg: f64,
    avg_request_size: f64,
    avg_response_size: f64,
    rps: f64,
    start_end_times: Vec<(Duration, Duration)>,
    latencies: Vec<Duration>,
    request_bytes: u64,
    response_bytes: u64,
}

impl Stats {
    fn fmt_detail(&self, f: &mut nojson::JsonObjectFormatter<'_, '_, '_>) -> std::fmt::Result {
        f.member(
            "request",
            nojson::object(|f| {
                f.member("count", self.request_count)?;
                f.member("avg_size", self.avg_request_size)
            }),
        )?;
        f.member(
            "response",
            nojson::object(|f| {
                f.member("ok_count", self.response_ok_count)?;
                f.member("error_count", self.response_error_count)?;
                f.member("avg_size", self.avg_response_size)
            }),
        )?;
        f.member("concurrency", self.max_concurrency)?;
        f.member(
            "latency",
            nojson::object(|f| {
                f.member("min", self.latency_min)?;
                f.member("p25", self.latency_p25)?;
                f.member("p50", self.latency_p50)?;
                f.member("p75", self.latency_p75)?;
                f.member("max", self.latency_max)
            }),
        )?;
        Ok(())
    }
}

impl nojson::DisplayJson for Stats {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("elapsed", self.duration.as_secs_f64())?;
            f.member("rps", self.rps)?;
            f.member("avg_latency", self.latency_avg)?;
            f.member("detail", nojson::object(|f| self.fmt_detail(f)))?;
            Ok(())
        })
    }
}

impl Stats {
    fn response_count(&self) -> usize {
        self.response_ok_count + self.response_error_count
    }

    fn finalize(&mut self) {
        self.duration = self
            .start_end_times
            .iter()
            .map(|(_, end)| *end)
            .max()
            .unwrap_or_default()
            .saturating_sub(
                self.start_end_times
                    .iter()
                    .map(|(start, _)| *start)
                    .min()
                    .unwrap_or_default(),
            );

        if self.duration > Duration::ZERO {
            let t = self.duration.as_secs_f64();
            self.rps = self.request_count as f64 / t;
        }

        if self.request_count > 0 {
            self.avg_request_size = self.request_bytes as f64 / self.request_count as f64;
        }

        if self.response_count() > 0 {
            self.avg_response_size = self.response_bytes as f64 / self.response_count() as f64;
        }

        if !self.latencies.is_empty() {
            self.latencies.sort();
            self.latency_min = self.latencies.first().expect("unreachable").as_secs_f64();
            self.latency_p25 = self.latencies[self.latencies.len() / 4].as_secs_f64();
            self.latency_p50 = self.latencies[self.latencies.len() / 2].as_secs_f64();
            self.latency_p75 = self.latencies[self.latencies.len() * 3 / 4].as_secs_f64();
            self.latency_max = self.latencies.last().expect("unreachable").as_secs_f64();
            self.latency_avg = (self.latencies.iter().sum::<Duration>()
                / self.latencies.len() as u32)
                .as_secs_f64();
        }

        self.start_end_times.sort();
        for i in 0..self.start_end_times.len() {
            let (start, _end) = self.start_end_times[i];
            let concurrency = self.start_end_times[..i]
                .iter()
                .rev()
                .take_while(|x| start < x.1)
                .count()
                + 1;
            self.max_concurrency = self.max_concurrency.max(concurrency);
        }
    }

    fn handle_output(
        &mut self,
        output: nojson::RawJsonValue<'_, '_>,
    ) -> Result<(), nojson::JsonParseError> {
        let Some(metadata) = output.to_member("metadata")?.get() else {
            return Ok(());
        };

        self.handle_metadata(metadata, output)?;

        self.request_count += 1;

        if output.to_member("result")?.get().is_some() {
            self.response_ok_count += 1;
        } else {
            self.response_error_count += 1;
        }

        Ok(())
    }

    fn handle_metadata(
        &mut self,
        metadata: nojson::RawJsonValue<'_, '_>,
        output: nojson::RawJsonValue<'_, '_>,
    ) -> Result<(), nojson::JsonParseError> {
        let start_time = Duration::from_micros(
            metadata
                .to_member("start_time_us")?
                .required()?
                .try_into()?,
        );
        let end_time =
            Duration::from_micros(metadata.to_member("end_time_us")?.required()?.try_into()?);
        self.start_end_times.push((start_time, end_time));
        self.latencies.push(end_time.saturating_sub(start_time));

        let request_bytes = metadata
            .to_member("request")?
            .required()?
            .as_raw_str()
            .len();
        self.request_bytes += request_bytes as u64;

        let response_bytes =
            output.as_raw_str().len() - (r#",\"metadata\":"#.len() + metadata.as_raw_str().len());
        self.response_bytes += response_bytes as u64;

        Ok(())
    }
}
