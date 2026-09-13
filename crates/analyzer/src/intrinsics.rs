use errors::error::{HydraError, Span};
use ir::intrinsic::IntrinsicKind;
use ir::types::Type as IRType;

use crate::Analyzer;

impl<'ctx> Analyzer<'ctx> {

    pub(crate) fn validate_intrinsic_signature(
        &self,
        kind: IntrinsicKind,
        generic_params: &[String],
        params: &[IRType],
        return_type: &IRType,
        span: Span,
    ) -> Result<(), HydraError> 
    {
        match kind {
            IntrinsicKind::SizeOf | IntrinsicKind::AlignOf => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "layout intrinsic requires exactly one type parameter",
                        span,
                    ));
                }

                if !params.is_empty() {
                    return Err(self.error(
                        "S017",
                        "layout intrinsic accepts no value parameters",
                        span,
                    ));
                }

                if *return_type != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "layout intrinsic must return `usize`",
                        span,
                    ));
                }
            }

            IntrinsicKind::PtrRead => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_read requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_read requires exactly one argument",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::CONST_POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_read expects `*const T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_read expects `*const T`",
                            span,
                        ));
                    }
                }

                match return_type {
                    IRType::GENERIC(name) if name == expected => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_read must return `T`",
                            span,
                        ));
                    }
                }
            }

            IntrinsicKind::PtrWrite => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_write requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 2 {
                    return Err(self.error(
                        "S017",
                        "ptr_write requires exactly two arguments",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_write expects first argument to be `*mut T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_write expects first argument to be `*mut T`",
                            span,
                        ));
                    }
                }

                match &params[1] {
                    IRType::GENERIC(name) if name == expected => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_write expects second argument to be `T`",
                            span,
                        ));
                    }
                }

                if *return_type != IRType::VOID {
                    return Err(self.error(
                        "S017",
                        "ptr_write must return `void`",
                        span,
                    ));
                }
            }

            IntrinsicKind::PtrOffset => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_offset requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 2 {
                    return Err(self.error(
                        "S017",
                        "ptr_offset requires exactly two arguments",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_offset expects first argument to be `*mut T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_offset expects first argument to be `*mut T`",
                            span,
                        ));
                    }
                }

                if params[1] != IRType::ISIZE {
                    return Err(self.error(
                        "S017",
                        "ptr_offset expects second argument to be `isize`",
                        span,
                    ));
                }

                match return_type {
                    IRType::POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_offset must return `*mut T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_offset must return `*mut T`",
                            span,
                        ));
                    }
                }
            }

            IntrinsicKind::PtrNull | IntrinsicKind::PtrNullMut => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "null pointer intrinsic requires exactly one type parameter",
                        span
                    ));
                }

                if !params.is_empty() {
                    return Err(self.error(
                        "S017",
                        "null pointer intrinsic accepts no value parameters",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                let matches_return = match (kind, return_type) {
                    (IntrinsicKind::PtrNull, IRType::CONST_POINTER(inner)) | 
                    (IntrinsicKind::PtrNullMut, IRType::POINTER(inner)) => {
                        matches!(inner.as_ref(), IRType::GENERIC(name) if name == expected)
                    }

                    _ => false,
                };

                if !matches_return {
                    let expected_return = match kind {
                        IntrinsicKind::PtrNull => "*const T",
                        IntrinsicKind::PtrNullMut => "*mut T",
                        _ => unreachable!(),
                    };

                    return Err(self.error(
                        "S017",
                        format!("null pointer intrinsic must return `{}`", expected_return),
                        span,
                    ));
                }
            }

            IntrinsicKind::PtrIsNull => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_is_null requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_is_null requires exactly one argument",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::CONST_POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => return Err(self.error(
                                "S017",
                                "ptr_is_null expects `*const T`",
                                span,
                            ))
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_is_null expects `*const T`",
                            span,
                        ));
                    }
                }

                if *return_type != IRType::BOOL {
                    return Err(self.error(
                        "S017",
                        "ptr_is_null must return `bool`",
                        span,
                    ));
                }
            }

            IntrinsicKind::PtrFromAddr => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_from_addr requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "ptr_from_addr requires exactly one argument",
                        span,
                    ));
                }

                if params[0] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "ptr_from_addr expects address to be `usize`",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match return_type {
                    IRType::POINTER(inner) => {
                        match inner.as_ref() {
                            IRType::GENERIC(name) if name == expected => {}

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "ptr_from_addr must return `*mut T`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "ptr_from_addr must return `*mut T`",
                            span,
                        ));
                    }
                }
            }

            IntrinsicKind::Alloc => {
                if !generic_params.is_empty() {
                    return Err(self.error(
                        "S017",
                        "alloc accepts no type parameters",
                        span,
                    ));
                }

                if params.len() != 2 {
                    return Err(self.error(
                        "S017",
                        "alloc requires exactly two arguments",
                        span,
                    ));
                }

                if params[0] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "alloc expects `size` to be `usize`",
                        span,
                    ));
                }

                if params[1] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "alloc expects `align` to be `usize`",
                        span,
                    ));
                }

                match return_type {
                    IRType::POINTER(inner)
                    if inner.as_ref() == &IRType::U8 => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "alloc must return `*mut u8`",
                            span,
                        ));
                    }
                }
            }

            IntrinsicKind::Dealloc => {
                if !generic_params.is_empty() {
                    return Err(self.error(
                        "S017",
                        "dealloc accepts no type parameters",
                        span,
                    ));
                }

                if params.len() != 3 {
                    return Err(self.error(
                        "S017",
                        "dealloc requires exactly three arguments",
                        span,
                    ));
                }

                match &params[0] {
                    IRType::POINTER(inner)
                    if inner.as_ref() == &IRType::U8 => {}

                    _ => {
                        return Err(self.error(
                            "S017",
                            "dealloc expects first argument to be `*mut u8`",
                            span,
                        ));
                    }
                }

                if params[1] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "dealloc expects `size` to be `usize`",
                        span,
                    ));
                }

                if params[2] != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "dealloc expects `align` to be `usize`",
                        span,
                    ));
                }

                if *return_type != IRType::VOID {
                    return Err(self.error(
                        "S017",
                        "dealloc must return `void`",
                        span,
                    ));
                }
            }

            IntrinsicKind::SliceLen => {
                if generic_params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "slice_len requires exactly one type parameter",
                        span,
                    ));
                }

                if params.len() != 1 {
                    return Err(self.error(
                        "S017",
                        "slice_len requires exactly one argument",
                        span,
                    ));
                }

                let expected = &generic_params[0];

                match &params[0] {
                    IRType::CONST_REF(inner) | IRType::REF(inner) => {
                        match inner.as_ref() {
                            IRType::SLICE(element) => {
                                match element.as_ref() {
                                    IRType::GENERIC(name) if name == expected => {}

                                    _ => {
                                        return Err(self.error(
                                            "S017",
                                            "slice_len expects `&[T]`",
                                            span,
                                        ));
                                    }
                                }
                            }

                            _ => {
                                return Err(self.error(
                                    "S017",
                                    "slice_len expects `&[T]`",
                                    span,
                                ));
                            }
                        }
                    }

                    _ => {
                        return Err(self.error(
                            "S017",
                            "slice_len expects `&[T]`",
                            span,
                        ));
                    }
                }

                if *return_type != IRType::USIZE {
                    return Err(self.error(
                        "S017",
                        "slice_len must return `usize`",
                        span,
                    ));
                }
            }
        }

        Ok(())
    }
}
