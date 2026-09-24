use crate::{options::CompilerOptions, sysroot::Sysroot, timing::Timings};

pub struct Session {
    pub options: CompilerOptions,
    pub sysroot: Sysroot,
    pub timings: Timings,
}

impl Session {
    pub fn new(options: CompilerOptions, sysroot: Sysroot) -> Self {
        let timings = Timings::new(options.time_passes, options.verbosity);

        Self {
            options,
            sysroot,
            timings,
        }
    }
}
