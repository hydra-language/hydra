#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntrinsicKind {
    SizeOf,
    AlignOf,
    PtrRead,
    PtrWrite,
    PtrOffset,
    Alloc,
    Dealloc,
}

impl IntrinsicKind {

    pub fn from_path(path: &[String]) -> Option<Self> {
        match path {
            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "size_of" =>
            {
                Some(Self::SizeOf)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "align_of" =>
            {
                Some(Self::AlignOf)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "ptr_read" =>
            {
                Some(Self::PtrRead)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "ptr_write" =>
            {
                Some(Self::PtrWrite)
            }

            [core, intrinsics, name]
                if core == "core" && intrinsics == "intrinsics" && name == "ptr_offset" =>
            {
                Some(Self::PtrOffset)
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


            _ => None,
        }
    }

    pub fn has_side_effects(self) -> bool {
        match self {
            IntrinsicKind::PtrWrite | IntrinsicKind::Alloc | IntrinsicKind::Dealloc => true,

            IntrinsicKind::SizeOf | IntrinsicKind::AlignOf | 
            IntrinsicKind::PtrRead | IntrinsicKind::PtrOffset => false,
        }
    }
}
