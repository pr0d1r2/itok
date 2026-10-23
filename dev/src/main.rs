fn main() {
    std::process::exit(i32::from(itok_dev::run(
        &std::env::args().collect::<Vec<_>>(),
        std::path::Path::new("."),
    )));
}
