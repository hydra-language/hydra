use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct PhaseTimer {
    name: &'static str,
    start: Instant,
}

#[derive(Debug)]
struct ItemTiming {
    name: String,
    duration: Duration,
}

#[derive(Debug, Default)]
pub struct Timings {
    enabled: bool,
    verbosity: u8,
    phases: Vec<(&'static str, Duration)>,
    items: HashMap<&'static str, Vec<ItemTiming>>,
}

impl Timings {

    pub fn new(enabled: bool, verbosity: u8) -> Self {
        Self {
            enabled,
            verbosity,
            phases: Vec::new(),
            items: HashMap::new(),
        }
    }

    pub fn start(&self, name: &'static str) -> Option<PhaseTimer> {
        self.enabled.then(|| PhaseTimer {
            name,
            start: Instant::now(),
        })
    }

    pub fn finish(&mut self, timer: Option<PhaseTimer>) {
        let Some(timer) = timer else {
            return;
        };

        self.phases.push((timer.name, timer.start.elapsed()));
    }

    pub fn record_item(&mut self, phase: &'static str, name: impl Into<String>, duration: Duration) {
        if !self.enabled {
            return;
        }

        self.items.entry(phase).or_default().push(ItemTiming {
            name: name.into(),
            duration,
        });
    }

    pub fn print(&self) {
        if !self.enabled {
            return;
        }

        eprintln!();
        eprintln!("pass timings:");

        for (name, duration) in &self.phases {
            eprintln!(
                "  {:<24} {:>9.3} ms",
                name,
                duration.as_secs_f64() * 1000.0,
            );

            self.print_items(name);
        }

        let total: Duration = self.phases.iter().map(|(_, duration)| *duration).sum();

        eprintln!(
            "  {:<24} {:>9.3} ms",
            "total",
            total.as_secs_f64() * 1000.0,
        );
    }

    fn print_items(&self, phase: &'static str) {
        if self.verbosity == 0 {
            return;
        }

        let Some(items) = self.items.get(phase) else {
            return;
        };

        if items.is_empty() {
            return;
        }

        let total: Duration = items.iter().map(|item| item.duration).sum();
        let average = total.as_secs_f64() * 1000.0 / items.len() as f64;

        eprintln!(
            "    {:<20} {:>6} functions, avg {:>8.3} ms",
            "",
            items.len(),
            average,
        );

        if self.verbosity < 2 {
            return;
        }

        let mut sorted: Vec<&ItemTiming> = items.iter().collect();

        sorted.sort_by(|lhs, rhs| {
            rhs.duration.cmp(&lhs.duration)
        });

        let limit = if self.verbosity >= 3 {
            sorted.len()
        } else {
            sorted.len().min(20)
        };

        for item in sorted.into_iter().take(limit) {
            eprintln!(
                "      {:<60} {:>9.3} ms",
                item.name,
                item.duration.as_secs_f64() * 1000.0,
            );
        }

        if self.verbosity == 2 && items.len() > limit {
            eprintln!(
                "      ... {} more functions",
                items.len() - limit,
            );
        }
    }
}
