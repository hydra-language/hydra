#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntrinsicKind {
    SizeOf,
    AlignOf,
    PtrRead,
    PtrWrite,
    PtrOffset,
    PtrNull,
    PtrNullMut,
    PtrIsNull,
    PtrFromAddr,
    Alloc,
    Dealloc,
    SliceLen,
}

impl IntrinsicKind {

    pub fn from_path(path: &[String]) -> Option<Self> {
        match path {
            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__size_of" =>
            {
                Some(Self::SizeOf)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__align_of" =>
            {
                Some(Self::AlignOf)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__ptr_read" =>
            {
                Some(Self::PtrRead)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__ptr_write" =>
            {
                Some(Self::PtrWrite)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__ptr_offset" =>
            {
                Some(Self::PtrOffset)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__ptr_null" =>
            {
                Some(Self::PtrNull)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__ptr_null_mut" =>
            {
                Some(Self::PtrNullMut)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__ptr_is_null" =>
            {
                Some(Self::PtrIsNull)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__ptr_from_addr" =>
            {
                Some(Self::PtrFromAddr)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__alloc" =>
            {
                Some(Self::Alloc)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__dealloc" =>
            {
                Some(Self::Dealloc)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "__slice_len" => 
            {
                Some(Self::SliceLen)
            }

            _ => None,
        }
    }

    pub fn has_side_effects(self) -> bool {
        match self {
            IntrinsicKind::PtrWrite | IntrinsicKind::Alloc | IntrinsicKind::Dealloc => true,

            IntrinsicKind::SizeOf | IntrinsicKind::AlignOf | 
            IntrinsicKind::PtrRead | IntrinsicKind::PtrOffset |
            IntrinsicKind::PtrNull | IntrinsicKind::PtrNullMut |
            IntrinsicKind::PtrIsNull | IntrinsicKind::PtrFromAddr |
            IntrinsicKind::SliceLen => false,
        }
    }
}
