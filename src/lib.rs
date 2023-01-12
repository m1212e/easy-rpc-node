#![deny(clippy::all)]

//TODO: remove unwraps
//TODO: refactoring


mod erpc;
mod web;
mod threadsafe_function;
mod server;
mod target;

#[macro_use]
extern crate napi_derive;


