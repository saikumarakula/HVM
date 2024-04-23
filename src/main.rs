#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

use clap::{Arg, ArgAction, Command};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command as SysCommand;

mod ast;
mod cmp;
mod hvm;

extern "C" {
  fn hvm_c(book_buffer: *const u32);
}

#[cfg(feature = "cuda")]
extern "C" {
  fn hvm_cu(book_buffer: *const u32);
}

fn main() {
  let matches = Command::new("kind2")
    .about("HVM2: Higher-order Virtual Machine 2 (32-bit Version)")
    .subcommand_required(true)
    .arg_required_else_help(true)
    .subcommand(Command::new("run").about("Interprets a file (using Rust)").arg(Arg::new("file").required(true)))
    .subcommand(Command::new("run-c").about("Interprets a file (using C)").arg(Arg::new("file").required(true)))
    .subcommand(Command::new("run-cu").about("Interprets a file (using CUDA)").arg(Arg::new("file").required(true)))
    .subcommand(Command::new("gen-c").about("Compiles a file (to standalone C)").arg(Arg::new("file").required(true)))
    .subcommand(Command::new("gen-cu").about("Compiles a file (to standalone CUDA)").arg(Arg::new("file").required(true)))
    .get_matches();

  match matches.subcommand() {
    Some(("run", sub_matches)) => {
      // Loads file/code/book
      let file = sub_matches.get_one::<String>("file").expect("required");
      let code = fs::read_to_string(file).expect("Unable to read file");
      let book = ast::Book::parse(&code).unwrap_or_else(|er| panic!("{}",er)).build();

      // Runs on interpreted mode
      run(&book);
    }
    Some(("run-c", sub_matches)) => {
      // Loads file/code/book
      let file = sub_matches.get_one::<String>("file").expect("required");
      let code = fs::read_to_string(file).expect("Unable to read file");
      let book = ast::Book::parse(&code).unwrap_or_else(|er| panic!("{}",er)).build();

      // Converts Book to buffer
      let mut data : Vec<u8> = Vec::new();
      book.to_buffer(&mut data);
      //println!("{:?}", data);

      unsafe {
        hvm_c(data.as_mut_ptr() as *mut u32);
      }
    }
    Some(("run-cu", sub_matches)) => {
      // Loads file/code/book
      let file = sub_matches.get_one::<String>("file").expect("required");
      let code = fs::read_to_string(file).expect("Unable to read file");
      let book = ast::Book::parse(&code).unwrap_or_else(|er| panic!("{}",er)).build();

      // Converts Book to buffer
      let mut data : Vec<u8> = Vec::new();
      book.to_buffer(&mut data);

      #[cfg(feature = "cuda")]
      unsafe {
        hvm_cu(data.as_mut_ptr() as *mut u32);
      }
      #[cfg(not(feature = "cuda"))]
      println!("CUDA not available!\n");
    }
    Some(("gen-c", sub_matches)) => {
      // Loads file/code/book
      let file = sub_matches.get_one::<String>("file").expect("required");
      let code = fs::read_to_string(file).expect("Unable to read file");
      let book = ast::Book::parse(&code).unwrap_or_else(|er| panic!("{}",er)).build();

      // Generates compiled functions
      let fns = cmp::compile_book(cmp::Target::C, &book);

      // Generates compiled C file
      let hvm_c = include_str!("hvm.c");
      let hvm_c = hvm_c.replace("///COMPILED_INTERACT_CALL///", &fns);
      let hvm_c = hvm_c.replace("#define INTERPRETED", "#define COMPILED");

      println!("{}", hvm_c);
    }
    Some(("gen-cu", sub_matches)) => {
      // Loads file/code/book
      let file = sub_matches.get_one::<String>("file").expect("required");
      let code = fs::read_to_string(file).expect("Unable to read file");
      let book = ast::Book::parse(&code).unwrap_or_else(|er| panic!("{}",er)).build();

      // Generates compiled functions
      let fns = cmp::compile_book(cmp::Target::CUDA, &book);

      // Generates compiled C file
      let hvm_c = include_str!("hvm.cu");
      let hvm_c = hvm_c.replace("///COMPILED_INTERACT_CALL///", &fns);
      let hvm_c = hvm_c.replace("#define INTERPRETED", "#define COMPILED");

      println!("{}", hvm_c);
    }
    _ => unreachable!(),
  }
}


pub fn run(book: &hvm::Book) {
  // Initializes the global net
  let net = hvm::GNet::new(1 << 29, 1 << 29);

  // Initializes threads
  let mut tm = hvm::TMem::new(0, 1);

  // Creates an initial redex that calls main
  let main_id = book.defs.iter().position(|def| def.name == "main").unwrap();
  tm.rbag.push_redex(hvm::Pair::new(hvm::Port::new(hvm::REF, main_id as u32), hvm::Port::new(hvm::VAR, 0)));
  net.vars_create(0, hvm::NONE);

  // Starts the timer
  let start = std::time::Instant::now();

  // Evaluates
  tm.evaluator(&net, &book);
  
  // Stops the timer
  let duration = start.elapsed();

  //println!("{}", net.show());

  // Prints the result
  if let Some(tree) = crate::ast::Tree::readback(&net, tm.enter(&net, hvm::Port::new(hvm::VAR,0)), &mut std::collections::BTreeMap::new(), &mut std::collections::BTreeMap::new()) {
    println!("Result: {}", tree.show());
  } else {
    println!("Readback failed. Printing GNet memdump...\n");
    println!("{}", net.show());
  }

  // Prints interactions and time
  let itrs = net.itrs.load(std::sync::atomic::Ordering::Relaxed);
  println!("- ITRS: {}", itrs);
  println!("- TIME: {:.2}s", duration.as_secs_f64());
  println!("- MIPS: {:.2}", itrs as f64 / duration.as_secs_f64() / 1_000_000.0);
}
