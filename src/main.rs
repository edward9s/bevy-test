fn main() {
    std::process::exit(if bevy_forest::run().is_success() {
        0
    } else {
        1
    });
}
