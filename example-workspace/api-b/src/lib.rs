#[cfg(feature = "capi")]
mod capi {
    #[no_mangle]
    extern "C" fn info() {
        eprintln!("C-API B");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
