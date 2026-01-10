use std::{io::BufRead, time::Duration};

use orfail::OrFail;

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("stats")
        .doc("Calculate statistics from JSON objects outputted by the bench command")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    if args.metadata().help_mode {
        return Ok(true);
    }

    run_stats()?;
    Ok(true)
}

fn run_stats() -> orfail::Result<()> {
    let stdin = std::io::stdin();
    let mut stats = Stats::default();

    let reader = stdin.lock();
    for line in reader.lines() {
        let line = line.or_fail()?;
        let json = nojson::RawJson::parse(&line).or_fail()?;
        stats.handle_output(json.value()).or_fail()?;
    }

    stats.latencies.sort_unstable();
    stats.start_end_times.sort_unstable();

    println!("{}", nojson::Json(&stats));
    Ok(())
}

#[derive(Debug, Default)]
struct Stats {
    success_count: usize,
    error_count: usize,
    start_end_times: Vec<(Duration, Duration)>,
    latencies: Vec<Duration>,
    request_bytes: u64,
    response_bytes: u64,
}

impl Stats {
    fn request_count(&self) -> usize {
        self.success_count + self.error_count
    }

    fn calculate_duration(&self) -> Duration {
        let min_start = self.start_end_times.iter().map(|(s, _)| *s).min();
        let max_end = self.start_end_times.iter().map(|(_, e)| *e).max();

        match (min_start, max_end) {
            (Some(start), Some(end)) => end.saturating_sub(start),
            _ => Duration::ZERO,
        }
    }

    fn calculate_rps(&self, duration: Duration) -> usize {
        let request_count = self.request_count();
        if duration > Duration::ZERO {
            let t = duration.as_secs_f64();
            (request_count as f64 / t).round() as usize
        } else {
            0
        }
    }

    fn calculate_avg_request_size(&self) -> f64 {
        let request_count = self.request_count();
        if request_count > 0 {
            self.request_bytes as f64 / request_count as f64
        } else {
            0.0
        }
    }

    fn calculate_avg_response_size(&self) -> f64 {
        let response_count = self.request_count();
        if response_count > 0 {
            self.response_bytes as f64 / response_count as f64
        } else {
            0.0
        }
    }

    fn calculate_latency_stats(&self) -> LatencyStats {
        if self.latencies.is_empty() {
            return LatencyStats::default();
        }

        let len = self.latencies.len();

        LatencyStats {
            min: self.latencies[0].as_secs_f64(),
            p25: self.latencies[len / 4].as_secs_f64(),
            p50: self.latencies[len / 2].as_secs_f64(),
            p75: self.latencies[len * 3 / 4].as_secs_f64(),
            max: self.latencies[len - 1].as_secs_f64(),
            avg: (self.latencies.iter().sum::<Duration>() / len as u32).as_secs_f64(),
        }
    }

    fn calculate_max_concurrency(&self) -> usize {
        let mut max_concurrency = 0;
        for i in 0..self.start_end_times.len() {
            let (start, _) = self.start_end_times[i];
            let concurrency = self.start_end_times[..i]
                .iter()
                .rev()
                .take_while(|(_, end)| start < *end)
                .count()
                + 1;
            max_concurrency = max_concurrency.max(concurrency);
        }

        max_concurrency
    }
}

impl nojson::DisplayJson for Stats {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.set_indent_size(2);
        f.set_spacing(true);

        let duration = self.calculate_duration();
        let rps = self.calculate_rps(duration);
        let latency_stats = self.calculate_latency_stats();
        let avg_request_size = self.calculate_avg_request_size();
        let avg_response_size = self.calculate_avg_response_size();
        let max_concurrency = self.calculate_max_concurrency();

        f.object(|f| {
            f.member("elapsed_seconds", duration.as_secs_f64())?;
            f.member("requests_per_second", rps)?;
            f.member("avg_latency_seconds", latency_stats.avg)?;
            f.member(
                "detail",
                nojson::object(|f| {
                    self.fmt_detail(
                        f,
                        &latency_stats,
                        avg_request_size,
                        avg_response_size,
                        max_concurrency,
                    )
                }),
            )?;
            Ok(())
        })
    }
}

impl Stats {
    fn fmt_detail(
        &self,
        f: &mut nojson::JsonObjectFormatter<'_, '_, '_>,
        latency_stats: &LatencyStats,
        avg_request_size: f64,
        avg_response_size: f64,
        max_concurrency: usize,
    ) -> std::fmt::Result {
        f.member(
            "count",
            no_indent_object(|f| {
                f.member("success", self.success_count)?;
                f.member("error", self.error_count)
            }),
        )?;
        f.member(
            "size",
            no_indent_object(|f| {
                f.member("request_avg_bytes", avg_request_size.round() as usize)?;
                f.member("response_avg_bytes", avg_response_size.round() as usize)
            }),
        )?;
        f.member(
            "latency",
            no_indent_object(|f| {
                f.member("min", latency_stats.min)?;
                f.member("p25", latency_stats.p25)?;
                f.member("p50", latency_stats.p50)?;
                f.member("p75", latency_stats.p75)?;
                f.member("max", latency_stats.max)
            }),
        )?;
        f.member(
            "concurrency",
            no_indent_object(|f| f.member("max", max_concurrency)),
        )?;
        Ok(())
    }

    fn handle_output(
        &mut self,
        output: nojson::RawJsonValue<'_, '_>,
    ) -> Result<(), nojson::JsonParseError> {
        // Extract timing and size information from root level
        let start_time_micros: u64 = output
            .to_member("start_unix_timestamp_micros")?
            .required()?
            .try_into()?;
        let end_time_micros: u64 = output
            .to_member("end_unix_timestamp_micros")?
            .required()?
            .try_into()?;
        let request_byte_size: usize = output
            .to_member("request_byte_size")?
            .required()?
            .try_into()?;
        let response_byte_size: usize = output
            .to_member("response_byte_size")?
            .required()?
            .try_into()?;

        let start_time = Duration::from_micros(start_time_micros);
        let end_time = Duration::from_micros(end_time_micros);

        self.start_end_times.push((start_time, end_time));
        self.latencies.push(end_time.saturating_sub(start_time));

        self.request_bytes += request_byte_size as u64;
        self.response_bytes += response_byte_size as u64;

        // Check for success/error based on presence of "result" or "error"
        if output.to_member("error")?.get().is_some() {
            self.error_count += 1;
        } else {
            output.to_member("result")?.required()?;
            self.success_count += 1;
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
struct LatencyStats {
    min: f64,
    p25: f64,
    p50: f64,
    p75: f64,
    max: f64,
    avg: f64,
}

fn no_indent_object<F>(f: F) -> impl nojson::DisplayJson
where
    F: Fn(&mut nojson::JsonObjectFormatter<'_, '_, '_>) -> std::fmt::Result,
{
    nojson::json(move |fmt| {
        fmt.set_indent_size(0);
        fmt.object(|fmt| f(fmt))?;
        fmt.set_indent_size(2);
        Ok(())
    })
}
