//! Module implementing GitHub as gist host.

use std::marker::PhantomData;

use gist;


#[derive(Debug)]
pub struct GitHub {
    _marker: PhantomData<()>,
}

impl GitHub {
    pub fn new() -> Self {
        GitHub { _marker: PhantomData }
    }
}

impl gist::Host for GitHub {
    fn name(&self) -> &str { "GitHub" }

    fn gists(&self, owner: &str) -> Vec<gist::Uri> {
        unimplemented!()
    }
}

const BASE_URL: &'static str = "http://gist.github.com";
