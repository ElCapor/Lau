use common::types::{Function, Instruction, Res};
use egui::{accesskit::Node, Color32};
use egui_snarl::{ui::SnarlViewer, InPinId, NodeId, OutPinId};
use lua54::{
	common::{inst::{Block, Condition, Control, Target}, types::Proto},
	dumper::dump_lua_module,
	loader::load_lua_module,
};
use rand::seq::SliceRandom;
use ron::{
	de::from_bytes,
	ser::{to_string_pretty, PrettyConfig},
};
use std::{
	arch::x86_64::__m128, collections::HashMap, hash::Hash, io::{Result, Write}, rc::Rc
};

mod common;
mod lua54;

enum Mutation {
	Random,
	Sorted,
}

fn try_mutate(func: &mut Function<Block>, opt: &[Mutation]) {
	let mut rng = rand::thread_rng();

	for data in &mut func.child_list {
		try_mutate(&mut data.1, opt);
	}

	for step in opt.iter() {
		match step {
			Mutation::Random => {
				func.block_list.shuffle(&mut rng);
				func.child_list.shuffle(&mut rng);
				func.upval_list.shuffle(&mut rng);
				func.value_list.shuffle(&mut rng);
			}
			Mutation::Sorted => {
				func.block_list.sort_by_key(|v| v.label);
				func.child_list.sort_by_key(|v| Rc::clone(&v.0));
				func.upval_list.sort_by_key(|v| Rc::clone(&v.0));
				func.value_list.sort_by_key(|v| Rc::clone(&v.0));
			}
		}
	}
}

fn assemble_data(data: &[u8], opt: &[Mutation]) -> Result<()> {
	let mut func = from_bytes(data).expect("not valid RON");

	try_mutate(&mut func, opt);

	let proto = Proto::from(func);
	let binary = dump_lua_module(&proto)?;

	std::io::stdout().lock().write_all(&binary)
}

fn disassemble_data(data: &[u8], opt: &[Mutation]) -> Result<()> {
	let (trail, proto) = load_lua_module(data).expect("not valid Lua 5.4 bytecode");

	if !trail.is_empty() {
		panic!("trailing garbage in Lua file");
	}

	let mut func = Function::from(proto);

	try_mutate(&mut func, opt);

	let config = PrettyConfig::new();
	let ron = to_string_pretty(&func, config).expect("not convertible to RON");

	std::io::stdout().lock().write_all(ron.as_bytes())
}

fn list_help() {
	println!("usage: lau [options]");
	println!("  -h | --help                show the help message");
	println!("  -a | --assemble [file]     assemble a RON file into bytecode");
	println!("  -d | --disassemble [file]  disassemble a bytecode file into RON");
	println!("  -r | --randomize           queue a randomization step");
	println!("  -ui                        start UI mode");
	println!("  -s | --sort                queue a sorting step");
}


/* NODES LOGIC */

/*
Current road map:
A flow node is just a label , with instructions, in the future the vector
of instructions is gonna be turned into a list of nodes
*/


/* UI APP LOGIC */
use eframe::egui;

struct BlocksViewer;

impl Block {
	fn name(&self) -> String {
		return format!("Block {}", self.label);
	}
}

impl SnarlViewer<Block> for BlocksViewer {
	fn title(&mut self, node: &Block) -> String {
		return node.name();
	}

	fn outputs(&mut self, node: &Block) -> usize {
		// depending on the edge, a block can be linked to zero , one or two nodes
		match node.edge {
			Control::Unconditional(_) => {
				return 1;
			}
			Control::Condition(_,_ ,_ ) => {
				return 2;
			}
			Control::Loop(_, _, _) => {
				return 2;
			}
			Control::Return(_, _, _, _)=> {
				return 0;
			}
			Control::Return0 => 
			{
				return 0;
			}
			Control::Return1(_) => 
			{
				return 0;
			}
			Control::LFalseSkip(_, _) =>
			{
				return 1;
			}
			_ => { return 0; }
		}
	}

	fn inputs(&mut self, node: &Block) -> usize {
		// a node always have 1 input
		return 1;
	}

	fn show_input(&mut self, pin: &egui_snarl::InPin, ui: &mut egui::Ui, scale: f32, snarl: &mut egui_snarl::Snarl<Block>)
		-> egui_snarl::ui::PinInfo {
		if let Some(block) = snarl.get_node(pin.id.node) {
			return egui_snarl::ui::PinInfo::circle().with_fill(Color32::from_rgb(255, 0, 0));
		} else {
			ui.label("Dead Input");
			return egui_snarl::ui::PinInfo::circle();
		}
	}

	fn show_output(
		&mut self,
		pin: &egui_snarl::OutPin,
		ui: &mut egui::Ui,
		scale: f32,
		snarl: &mut egui_snarl::Snarl<Block>,
	) -> egui_snarl::ui::PinInfo {
		if let Some(block) = snarl.get_node(pin.id.node) {
			match block.edge{
				Control::Unconditional(_) =>
				{
					ui.label("Unconditional");
					return egui_snarl::ui::PinInfo::star();
				}
				Control::Condition(_, _, _) => {
					ui.label("Conditional");
					return egui_snarl::ui::PinInfo::square();
				}
				Control::Loop(_, _, _) | Control::LFalseSkip(_, _) => {
					ui.label("Loop");
					return egui_snarl::ui::PinInfo::circle();
				}
				_ => {
					// no render for return anyways
					ui.label("Unknown");
					return egui_snarl::ui::PinInfo::circle();
				}
			}
		} else {
			ui.label("Dead output");
			return egui_snarl::ui::PinInfo::circle();
		}
	}
}

struct EApp {
	snarl : egui_snarl::Snarl<Block>,
	snarl_ui_id: Option<egui::Id>,
	style: egui_snarl::ui::SnarlStyle,
	file_path :String
}

impl EApp {
	pub fn new(cc :&eframe::CreationContext<'_>) -> Self
	{
		let mut snarl: egui_snarl::Snarl<Block> = egui_snarl::Snarl::new();
		let style = egui_snarl::ui::SnarlStyle::new();

		snarl.insert_node(egui::pos2(0.0, 0.0), Block::new(1, Vec::new(), Control::Unconditional(Target::Label(10))));
		let file_path = String::new();
		return EApp{
			snarl,
			snarl_ui_id:None,
			style,
			file_path
		}
	}

	pub fn set_file(&mut self, fl :String) -> ()
	{
		self.file_path = fl;
	}

	pub fn parse_ron_data(&mut self) -> ()
	{
		let data = std::fs::read(&self.file_path).expect("Incorrect file path");
		let func :Function<Block> = from_bytes(&data).expect("Invalid RON Data");
		let mut map : HashMap<u32, NodeId> = HashMap::new();

		// farm the data
		for block in func.block_list {
			let block_lbl = block.label;
			let id = self.snarl.insert_node(egui::pos2(0.0, 0.0), block);
			map.insert(block_lbl, id);
		}

		let data = std::fs::read(&self.file_path).expect("Incorrect file path");
		let func :Function<Block> = from_bytes(&data).expect("Invalid RON Data");

		for block in func.block_list {
			match &block.edge {
				Control::Unconditional(target) => {
					match target {
						Target::Label(to_label) => {
							let node_from = map.get(&block.label);
							let node_to = map.get(to_label);
							if node_to.is_some() && node_from.is_some() {
								let out_pin : OutPinId =  OutPinId {
									node: *node_from.unwrap(),
									output: 0
								};
	
								let in_pin : InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0
								};
								self.snarl.connect(out_pin, in_pin);
							}
						}
						
						_ => {}
					}
				},
				Control::Condition(condition	,true_target ,false_target ) => {
					match true_target {
						Target::Label(to_label) => {
							let node_from = map.get(&block.label);
							let node_to = map.get(to_label);
							if node_to.is_some() && node_from.is_some() {
								let out_pin : OutPinId =  OutPinId {
									node: *node_from.unwrap(),
									output: 0
								};
	
								let in_pin : InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0 // true target is the first pin
								};
								self.snarl.connect(out_pin, in_pin);
							}
						}
						_ => {}
					}
					match false_target {
						Target::Label(to_label) => {
							let node_from = map.get(&block.label);
							let node_to = map.get(to_label);
							if node_to.is_some() && node_from.is_some() {
								let out_pin : OutPinId =  OutPinId {
									node: *node_from.unwrap(),
									output: 1 // false target is the 2nd pin
								};
	
								let in_pin : InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0
								};
								self.snarl.connect(out_pin, in_pin);
							}
						}
						_ => {}
					}
				}
				// loop has a reversed target order where on_false mean jmp back
				// and on_true means exit loop, but we restore the order back
				// on the node like a conditional
				Control::Loop(_, on_false, on_true) => {
					match on_true {
						Target::Label(to_label) => {
							let node_from = map.get(&block.label);
							let node_to = map.get(to_label);
							if node_to.is_some() && node_from.is_some() {
								let out_pin : OutPinId =  OutPinId {
									node: *node_from.unwrap(),
									output: 0
								};
	
								let in_pin : InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0 // true target is the first pin
								};
								self.snarl.connect(out_pin, in_pin);
							}
						}
						_ => {}
					}

					match on_false {
						Target::Label(to_label) => {
							let node_from = map.get(&block.label);
							let node_to = map.get(to_label);
							if node_to.is_some() && node_from.is_some() {
								let out_pin : OutPinId =  OutPinId {
									node: *node_from.unwrap(),
									output: 1 // false target is the 2nd pin
								};
	
								let in_pin : InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0
								};
								self.snarl.connect(out_pin, in_pin);
							}
						}

						_ => {}
					}
				}
				_ => {}
			}
		}


	}
}

impl Default for EApp {
	fn default() -> Self {
		let snarl: egui_snarl::Snarl<Block> = egui_snarl::Snarl::new();
		let style = egui_snarl::ui::SnarlStyle::new();
		let file_path = String::new();

		Self {
			snarl,
			snarl_ui_id:None,
			style,
			file_path
		}
	}
}

impl eframe::App for EApp {
	fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
		egui::CentralPanel::default().show(ctx, |ui| {
			ui.heading("Lau - The ultimate Lua ToolKit");

            ui.label(format!("Selected File: {}", &self.file_path));

			if ui.button("parse").clicked() {
				self.parse_ron_data();
			}
			
			self.snarl.show(&mut BlocksViewer, &self.style, "snarl", ui);
		});
	}

	
}


fn ui_mode(file_path :String) -> Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "LAU | dispatch fork",
        options,
        Box::new(|cc| {
			let mut app = EApp::new(cc);
			app.set_file(file_path);
			let ret = Box::new(app);
			return Ok(ret);
			
		}), // Cast EApp to Box<dyn App>
    ).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) // Convert eframe::Error to std::io::Error
}

fn main() -> Result<()> {
	let mut iter = std::env::args().skip(1);
	let mut mutation = Vec::new();

	while let Some(val) = iter.next() {
		match val.as_str() {
			"-h" | "--help" => {
				list_help();
			}
			"-a" | "--assemble" => {
				let name = iter.next().expect("file name expected");
				let data = std::fs::read(name)?;

				assemble_data(&data, &mutation)?;
			}
			"-d" | "--disassemble" => {
				let name = iter.next().expect("file name expected");
				let data = std::fs::read(name)?;

				disassemble_data(&data, &mutation)?;
			}
			"-r" | "--randomize" => {
				mutation.push(Mutation::Random);
			}
			"-s" | "--sort" => {
				mutation.push(Mutation::Sorted);
			}
			"-ui" => {
				let name = iter.next().expect("file name expected");

				ui_mode(name);
			}
			opt => {
				panic!("unknown option `{}`", opt);
			}
		}
	}

	Ok(())
}
