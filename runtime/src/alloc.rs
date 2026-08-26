#[unsafe(no_mangle)]
pub unsafe extern "C" fn hydra_alloc(size: usize, align: usize) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align_unchecked(
        size,
        align,
    );

    std::alloc::alloc(layout)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hydra_dealloc(ptr: *mut u8, size: usize, align: usize) {
    let layout = std::alloc::Layout::from_size_align_unchecked(
        size,
        align,
    );

    std::alloc::dealloc(ptr, layout);
}
