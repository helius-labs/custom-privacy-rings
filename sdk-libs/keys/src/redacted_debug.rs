pub const REDACTED: &str = "<redacted>";

/// `Debug` of a struct whose every field is listed, a field left out fails to compile.
#[macro_export]
macro_rules! redacted_debug {
    ($name:ident $(<$($lifetime:lifetime),+>)? {
        $(shown: [$($shown:ident),* $(,)?] $(,)?)?
        $(present: [$($present:ident),* $(,)?] $(,)?)?
        $(hidden: [$($hidden:ident),* $(,)?] $(,)?)?
    }) => {
        impl ::core::fmt::Debug for $name $(<$($lifetime),+>)? {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let Self {
                    $($($shown,)*)?
                    $($($present,)*)?
                    $($($hidden: _,)*)?
                } = self;
                formatter
                    .debug_struct(::core::stringify!($name))
                    $($(.field(::core::stringify!($shown), $shown))*)?
                    $($(.field(
                        ::core::stringify!($present),
                        &$present.as_ref().map(|_| $crate::REDACTED),
                    ))*)?
                    $($(.field(::core::stringify!($hidden), &$crate::REDACTED))*)?
                    .finish()
            }
        }
    };
}

#[cfg(test)]
mod tests {
    struct Sealed<'a> {
        ring: &'a str,
        list_salt: Option<[u8; 32]>,
        ring_salt: Option<[u8; 32]>,
        blinding: [u8; 32],
    }

    redacted_debug!(Sealed<'_> {
        shown: [ring],
        present: [list_salt, ring_salt],
        hidden: [blinding],
    });

    #[test]
    fn prints_shown_fields_and_marks_the_rest() {
        let sealed = Sealed {
            ring: "ring",
            list_salt: Some([7; 32]),
            ring_salt: None,
            blinding: [9; 32],
        };
        let printed = format!("{sealed:?}");
        assert!(!printed.contains(&format!("{:?}", sealed.blinding)));
        assert_eq!(
            printed,
            r#"Sealed { ring: "ring", list_salt: Some("<redacted>"), ring_salt: None, blinding: "<redacted>" }"#
        );
    }
}
