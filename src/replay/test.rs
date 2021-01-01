#![feature(negative_impls)]
struct Foo {
    i: u64
}

impl !Unpin for Foo {}

async fn baz() {
}
async fn bar() {
    let mut _foo = Foo { i: 64 };
    baz().await;
    let _bar = _foo;
}

fn main() {}
