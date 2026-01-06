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

    let count: bool = noargs::flag("count")
        .doc("Include the `count` field in the resulting JSON object")
        .take(args)
        .is_present();
    let bps: bool = noargs::flag("bps")
        .doc("Include the `bps` field in the resulting JSON object")
        .take(args)
        .is_present();

    if args.metadata().help_mode {
        return Ok(false);
    }

    run_stats(count, bps)?;
    Ok(true)
}

fn run_stats(count: bool, bps: bool) -> orfail::Result<()> {
    let stdin = std::io::stdin();
    let mut stats = Stats::default();
    if count {
        stats.count = Some(Counter::default());
    }
    if bps {
        stats.bps = Some(Bps::default());
    }

    let reader = std::io::BufReader::new(stdin.lock());
    for line in reader.lines() {
        let line = line.or_fail()?;
        let json = nojson::RawJson::parse(&line).or_fail()?;
        stats.handle_output2(json.value()).or_fail()?;
    }
    stats.finalize();
    println!("{}", nojson::Json(&stats));
    Ok(())
}

#[derive(Debug, Default)]
struct Stats {
    request_count: usize,
    duration: Duration,
    max_concurrency: usize,
    count: Option<Counter>,
    rps: f64,
    bps: Option<Bps>,
    latency: Latency,

    // NOTE: The following fields are only used for internal computation
    start_end_times: Vec<(Duration, Duration)>,
    latencies: Vec<Duration>,
    outgoing_bytes: u64,
    incoming_bytes: u64,
}

impl Stats {
    fn fmt_detail(&self, f: &mut nojson::JsonObjectFormatter<'_, '_, '_>) -> std::fmt::Result {
        f.member("request", ())?;
        f.member("response", ())?;
        f.member("concurrency", self.max_concurrency)?;
        f.member(
            "latency",
            nojson::object(|f| {
                f.member("min", self.latency.min)?;
                f.member("p25", self.latency.p25)?;
                f.member("p50", self.latency.p50)?;
                f.member("p75", self.latency.p75)?;
                f.member("max", self.latency.max)
            }),
        )?;
        Ok(())
    }
}

impl nojson::DisplayJson for Stats {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            // - elapsed
            // - rps
            // - avg_latency
            // - detail
            //   - request { count, avg_size }
            //   - response { success_count, error_count, avg_size }
            //   - concurrency
            //   - latency {min, p25, p50, p75, max}

            f.member("elapsed", self.duration.as_secs_f64())?;
            f.member("rps", self.rps)?;
            f.member("avg_latency", self.latency.avg)?;
            f.member("detail", nojson::object(|f| self.fmt_detail(f)))?;

            // old
            if let Some(counter) = &self.count {
                f.member("count", counter)?;
            }

            if let Some(bps) = &self.bps {
                f.member("bps", bps)?;
            }

            f.member("latency", &self.latency)
        })
    }
}

impl Stats {
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
            if let Some(bps) = &mut self.bps {
                bps.incoming = (self.incoming_bytes * 8) as f64 / t;
                bps.outgoing = (self.outgoing_bytes * 8) as f64 / t;
            }

            self.rps = self.request_count as f64 / t;
        }

        if !self.latencies.is_empty() {
            self.latencies.sort();
            self.latency.min = self.latencies.first().expect("unreachable").as_secs_f64();
            self.latency.p25 = self.latencies[self.latencies.len() / 4].as_secs_f64();
            self.latency.p50 = self.latencies[self.latencies.len() / 2].as_secs_f64();
            self.latency.p75 = self.latencies[self.latencies.len() * 3 / 4].as_secs_f64();
            self.latency.max = self.latencies.last().expect("unreachable").as_secs_f64();
            self.latency.avg = (self.latencies.iter().sum::<Duration>()
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

    fn handle_output2(
        &mut self,
        output: nojson::RawJsonValue<'_, '_>,
    ) -> Result<(), nojson::JsonParseError> {
        self.request_count += 1;

        let metadata = output.to_member("metadata")?.get();
        if let Some(metadata) = metadata {
            self.handle_metadata2(metadata, output)?;
        }

        if let Some(counter) = &mut self.count {
            counter.requests += 1;

            if output.to_member("result")?.get().is_some() {
                counter.responses.ok += 1;
            } else {
                counter.responses.error += 1;
            }

            if metadata.is_none() {
                counter.missing_metadata_calls += 1;
            }
        }

        Ok(())
    }

    fn handle_metadata2(
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
        self.outgoing_bytes += request_bytes as u64;

        let response_bytes =
            output.as_raw_str().len() - (r#","metadata":"#.len() + metadata.as_raw_str().len());
        self.incoming_bytes += response_bytes as u64;

        Ok(())
    }
}

#[derive(Debug, Default)]
struct Counter {
    missing_metadata_calls: usize,
    requests: usize,
    responses: OkOrError,
}

impl nojson::DisplayJson for Counter {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("missing_metadata_calls", self.missing_metadata_calls)?;
            f.member("requests", self.requests)?;
            f.member("responses", &self.responses)
        })
    }
}

#[derive(Debug, Default)]
struct OkOrError {
    ok: usize,
    error: usize,
}

impl nojson::DisplayJson for OkOrError {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("ok", self.ok)?;
            f.member("error", self.error)
        })
    }
}

#[derive(Debug, Default)]
struct Latency {
    min: f64,
    p25: f64,
    p50: f64,
    p75: f64,
    max: f64,
    avg: f64,
}

impl nojson::DisplayJson for Latency {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("min", self.min)?;
            f.member("p25", self.p25)?;
            f.member("p50", self.p50)?;
            f.member("p75", self.p75)?;
            f.member("max", self.max)?;
            f.member("avg", self.avg)
        })
    }
}

#[derive(Debug, Default)]
struct Bps {
    outgoing: f64,
    incoming: f64,
}

impl nojson::DisplayJson for Bps {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("outgoing", self.outgoing)?;
            f.member("incoming", self.incoming)
        })
    }
}
