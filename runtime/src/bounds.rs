#[unsafe(no_mangle)]
pub extern "C" fn hydra_bounds_check_fail(index: usize, len: usize) -> ! {
    eprintln!(
        "index out of bounds: the len is {} but the index is {}",
        len,
        index,
    );

    std::process::exit(101);
}
