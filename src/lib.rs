//! IP address watching. Currently only implemented on Linux, and currently limited to synchronous operation.

#![deny(
    exceeding_bitshifts,
    invalid_type_param_default,
    missing_fragment_specifier,
    mutable_transmutes,
    no_mangle_const_items,
    overflowing_literals,
    patterns_in_fns_without_body,
    pub_use_of_private_extern_crate,
    unknown_crate_types,
    const_err,
    order_dependent_trait_objects,
    illegal_floating_point_literal_pattern,
    improper_ctypes,
    late_bound_lifetime_arguments,
    non_camel_case_types,
    non_shorthand_field_patterns,
    non_snake_case,
    non_upper_case_globals,
    no_mangle_generic_items,
    path_statements,
    private_in_public,
    stable_features,
    type_alias_bounds,
    tyvar_behind_raw_pointer,
    unconditional_recursion,
    unused,
    unused_allocation,
    unused_comparisons,
    unused_mut,
    unreachable_pub,
    anonymous_parameters,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    clippy::all
)]
#![forbid(
    intra_doc_link_resolution_failure,
    safe_packed_borrows,
    while_true,
    elided_lifetimes_in_paths,
    bare_trait_objects
)]
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
#[cfg(not(any(unix, windows)))]
compile_error!("Only Unix and Windows are supported");

/// An address change event
#[derive(Copy, Clone, Hash, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Event {
    /// A new local address has been added
    New(std::net::IpAddr),
    /// A local address has been deleted
    Delete(std::net::IpAddr),
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        let set = AddrSet::new();
        println!("Got event {:?}", set);
        for i in set.unwrap() {
            println!("Got event {:?}", i.unwrap())
        }
    }
}
