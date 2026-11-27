/// A fieldless enum spelled by name, `ALL` in declaration order.
macro_rules! named_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident($kind:literal) {
        $($(#[$doc:meta])* $variant:ident = $text:literal),+ $(,)?
    }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(serde_with::SerializeDisplay, serde_with::DeserializeFromStr)]
        $vis enum $name {
            $($(#[$doc])* $variant),+
        }

        impl $name {
            pub const ALL: [Self; [$($text),+].len()] = [$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::macros::UnknownName;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .into_iter()
                    .find(|named| named.as_str() == text)
                    .ok_or_else(|| $crate::macros::UnknownName {
                        kind: $kind,
                        name: text.to_owned(),
                    })
            }
        }

        impl clap::ValueEnum for $name {
            fn value_variants<'a>() -> &'a [Self] {
                &Self::ALL
            }

            fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
                Some(clap::builder::PossibleValue::new(self.as_str()))
            }
        }

    };
}

pub(crate) use named_enum;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} {name}")]
pub struct UnknownName {
    pub kind: &'static str,
    pub name: String,
}
