macro_rules! size_of {
    ($t: ty) => {
        ::std::mem::size_of::<$t>()
    };
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "bsd")]
mod bsd;
#[cfg(target_os = "bsd")]
pub use bsd::*;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
