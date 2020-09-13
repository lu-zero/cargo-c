#![warn(rust_2018_idioms)]

#[cfg(cargo_c)]
mod capi;

pub fn hello(name: &str) {
    println!("Hello, {}!", name);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
