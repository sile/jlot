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
    fn response_count(&self) -> usize {
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
        let request_count = self.response_count();
        if duration > Duration::ZERO {
            let t = duration.as_secs_f64();
            (request_count as f64 / t).round() as usize
        } else {
            0
        }
    }

    fn calculate_avg_request_size(&self) -> f64 {
        let request_count = self.response_count();
        if request_count > 0 {
            self.request_bytes as f64 / request_count as f64
        } else {
            0.0
        }
    }

    fn calculate_avg_response_size(&self) -> f64 {
        let response_count = self.response_count();
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

        let mut sorted_latencies = self.latencies.clone();
        sorted_latencies.sort_unstable();
        let len = sorted_latencies.len();

        LatencyStats {
            min: sorted_latencies[0].as_secs_f64(),
            p25: sorted_latencies[len / 4].as_secs_f64(),
            p50: sorted_latencies[len / 2].as_secs_f64(),
            p75: sorted_latencies[len * 3 / 4].as_secs_f64(),
            max: sorted_latencies[len - 1].as_secs_f64(),
            avg: (sorted_latencies.iter().sum::<Duration>() / len as u32).as_secs_f64(),
        }
    }

    fn calculate_max_concurrency(&self) -> usize {
        let mut sorted_times = self.start_end_times.clone();
        sorted_times.sort_unstable();

        let mut max_concurrency = 0;
        for i in 0..sorted_times.len() {
            let (start, _) = sorted_times[i];
            let concurrency = sorted_times[..i]
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
            f.member("avg_latency", latency_stats.avg)?;
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
            nojson::json(|f| {
                f.set_indent_size(0);
                f.object(|f| {
                    f.member("success", self.success_count)?;
                    f.member("error", self.error_count)
                })?;
                f.set_indent_size(2);
                Ok(())
            }),
        )?;
        f.member(
            "size",
            nojson::json(|f| {
                f.set_indent_size(0);
                f.object(|f| {
                    f.member("request_avg_bytes", avg_request_size)?;
                    f.member("response_avg_bytes", avg_response_size)
                })?;
                f.set_indent_size(2);
                Ok(())
            }),
        )?;
        f.member(
            "latency",
            nojson::json(|f| {
                f.set_indent_size(0);
                f.object(|f| {
                    f.member("min", latency_stats.min)?;
                    f.member("p25", latency_stats.p25)?;
                    f.member("p50", latency_stats.p50)?;
                    f.member("p75", latency_stats.p75)?;
                    f.member("max", latency_stats.max)
                })?;
                f.set_indent_size(2);
                Ok(())
            }),
        )?;
        f.member(
            "concurrency",
            nojson::json(|f| {
                f.set_indent_size(0);
                f.object(|f| f.member("max", max_concurrency))?;
                f.set_indent_size(2);
                Ok(())
            }),
        )?;
        Ok(())
    }

    fn handle_output(
        &mut self,
        output: nojson::RawJsonValue<'_, '_>,
    ) -> Result<(), nojson::JsonParseError> {
        let Some(metadata) = output.to_member("metadata")?.get() else {
            return Ok(());
        };

        self.handle_metadata(metadata, output)?;

        if output.to_member("result")?.get().is_some() {
            self.success_count += 1;
        } else {
            self.error_count += 1;
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

#[derive(Debug, Default)]
struct LatencyStats {
    min: f64,
    p25: f64,
    p50: f64,
    p75: f64,
    max: f64,
    avg: f64,
}
