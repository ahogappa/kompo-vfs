use cc;

fn main() {
    if cfg!(test) {
        println!("cargo::rerun-if-changed=./dummy_fs.c");
        cc::Build::new().file("./dummy_fs.c").compile("dummy_fs.o");
    }
}
