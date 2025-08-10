macro_rules! inner_attr_to_each {
    { #!$attr:tt $($it:item)* } => {
        $(
            #$attr
            $it
        )*
    }
}

pub(crate) mod error;
pub(crate) mod graph;
pub(crate) mod orchestrator;
pub(crate) mod pipeline_runner;
pub(crate) mod store;
pub(crate) mod util;
pub(crate) mod validation;

inner_attr_to_each! {
    #![cfg(feature = "default")]
    pub(crate) mod crypto;
    pub(crate) mod model;
    pub(crate) mod operator;
}

#[cfg(feature = "test")]
inner_attr_to_each! {
    #![cfg_attr(
        feature = "test",
        allow(
            missing_docs,
            clippy::missing_errors_doc,
            clippy::missing_panics_doc,
            reason = "Documentation not necessary since private API.",
        ),
    )]
    pub mod crypto;
    pub mod model;
    pub mod operator;
}
