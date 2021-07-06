use actix::prelude::*;

pub struct Mongo {}

impl Actor for Mongo {}

impl Supervised for Mongo {}

impl SystemService for Mongo {}
