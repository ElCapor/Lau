use common::types::{Function, Instruction, Res};
use egui::Color32;
use egui_snarl::{ui::SnarlViewer, InPinId, NodeId, OutPinId};
use lua54::{
	common::{
		inst::{Block, Condition, Control, Target},
		types::Proto,
	},
	dumper::dump_lua_module,
	loader::load_lua_module,
};
use rand::seq::SliceRandom;
use ron::{
	de::from_bytes,
	ser::{to_string_pretty, PrettyConfig},
};
use std::{
	arch::x86_64::__m128,
	collections::{HashMap, HashSet, VecDeque},
	hash::Hash,
	io::{Result, Write},
	rc::Rc,
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
	println!("  -v | --devirt              devritualize a RON file made by vsecure");
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

	fn is_unconditionnal(&self) -> bool {
		match &self.edge {
			Control::Unconditional(_) => {
				return true;
			}

			_ => {
				return false;
			}
		}
	}

	fn get_target_labels(&self) -> Vec<u32> {
		let mut ret: Vec<u32> = Vec::new();

		match &self.edge {
			Control::Condition(_, on_true, on_false) => {
				match on_true {
					Target::Label(to_label) => {
						ret.push(*to_label);
					}
					_ => {}
				}

				match on_false {
					Target::Label(to_label) => {
						ret.push(*to_label);
					}

					_ => {}
				}
			}

			Control::Unconditional(target) => match target {
				Target::Label(to_label) => {
					ret.push(*to_label);
				}

				_ => {}
			},

			Control::Loop(_, on_false, on_true) => {
				match on_true {
					Target::Label(to_label) => {
						ret.push(*to_label);
					}
					_ => {}
				}

				match on_false {
					Target::Label(to_label) => {
						ret.push(*to_label);
					}

					_ => {}
				}
			}

			Control::LFalseSkip(_, target) => match target {
				Target::Label(to_label) => {
					ret.push(*to_label);
				}

				_ => {}
			},
			_ => {}
		}

		ret
	}

	fn target_labels_to_nodeid(&self, node_map: HashMap<u32, NodeId>) -> Vec<NodeId> {
		let mut ret: Vec<NodeId> = Vec::new();

		let map = node_map;

		for target in self.get_target_labels() {
			if let Some(tg_id) = map.get(&target) {
				ret.push(*tg_id);
			}
		}

		ret
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
			Control::Condition(_, _, _) => {
				return 2;
			}
			Control::Loop(_, _, _) => {
				return 2;
			}
			Control::Return(_, _, _, _) => {
				return 0;
			}
			Control::Return0 => {
				return 0;
			}
			Control::Return1(_) => {
				return 0;
			}
			Control::LFalseSkip(_, _) => {
				return 1;
			}
			_ => {
				return 0;
			}
		}
	}

	fn inputs(&mut self, node: &Block) -> usize {
		// a node always have 1 input
		return 1;
	}

	fn show_input(
		&mut self,
		pin: &egui_snarl::InPin,
		ui: &mut egui::Ui,
		scale: f32,
		snarl: &mut egui_snarl::Snarl<Block>,
	) -> egui_snarl::ui::PinInfo {
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
			match block.edge {
				Control::Unconditional(_) => {
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
	snarl: egui_snarl::Snarl<Block>,
	snarl_ui_id: Option<egui::Id>,
	style: egui_snarl::ui::SnarlStyle,
	file_path: String,
	node_map: HashMap<u32, NodeId>,
}

impl EApp {
	pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
		let mut snarl: egui_snarl::Snarl<Block> = egui_snarl::Snarl::new();
		let style = egui_snarl::ui::SnarlStyle::new();

		snarl.insert_node(
			egui::pos2(0.0, 0.0),
			Block::new(1, Vec::new(), Control::Unconditional(Target::Label(10))),
		);
		let file_path = String::new();
		let node_map = HashMap::new();
		return EApp {
			snarl,
			snarl_ui_id: None,
			style,
			file_path,
			node_map,
		};
	}

	pub fn set_file(&mut self, fl: String) -> () {
		self.file_path = fl;
	}

	fn assign_node_levels(&mut self) -> HashMap<NodeId, u32> {
		let mut levels: HashMap<NodeId, u32> = HashMap::new();
		let mut visited: HashSet<NodeId> = HashSet::new();
		let mut queue: VecDeque<(NodeId, u32)> = VecDeque::new();

		let map = self.node_map.clone();

		// Start traversal from the first node (assuming it's labeled 0 or choose another)
		queue.push_back((*map.get(&0).unwrap(), 0));

		while let Some((node_id, level)) = queue.pop_front() {
			if !visited.insert(node_id) {
				continue; // Skip already visited nodes
			}

			levels.insert(node_id, level);

			// Retrieve node outputs and add unvisited neighbors to the queue
			if let Some(edge) = self.snarl.get_node(node_id) {
				for child_edge in edge.get_target_labels() {
					if let Some(target_node) = map.get(&child_edge) {
						if !visited.contains(&target_node) {
							queue.push_back((*target_node, level + 1));
						}
					}
				}
			}
		}

		levels
	}

	fn build_tree(
		&mut self,
		visited: &mut HashSet<NodeId>,
		root_idx: NodeId,
		start_row: usize,
		start_col: usize,
	) -> usize {
		const ROW_DIST: usize = 10;
		const NODE_DIST: usize = 100;

		let x = start_row * ROW_DIST;
		let y = start_col * NODE_DIST;
		let mut max_col = start_col;

		if let Some(blk) = self.snarl.get_node_info_mut(root_idx) {
			blk.pos = egui::pos2(x as f32, y as f32);
			let node_ids = blk.value.target_labels_to_nodeid(self.node_map.clone());
			node_ids.iter().enumerate().for_each(|(i, node_id)| {
				if visited.contains(node_id) {
					return;
				}

				visited.insert(*node_id);

				// calculate node row :
				let rs = 100 * i / node_ids.len();

				let curr_max_col = self.build_tree(visited, *node_id, rs, start_col + 2 * i);
				if curr_max_col > max_col {
					max_col = curr_max_col;
				}
			});
		}

		max_col
	}

	fn ranker_v2(&mut self) -> () {
		let mut visited: HashSet<NodeId> = HashSet::new();
		let mut max_col = 0;

		let map = self.node_map.clone();
		let root_idx = *map.get(&0).unwrap();
		visited.insert(root_idx);

		let curr_max_col = self.build_tree(&mut visited, root_idx, 0, 0);

		if curr_max_col > max_col {
			max_col = curr_max_col;
		};
	}

	pub fn populate_map(&mut self) -> () {
		let data = std::fs::read(&self.file_path).expect("Incorrect file path");
		let func: Function<Block> = from_bytes(&data).expect("Invalid RON Data");
		let mut map: HashMap<u32, NodeId> = HashMap::new();

		// farm the data
		for block in func.block_list {
			let block_lbl = block.label;
			let id = self.snarl.insert_node(egui::pos2(0.0, 0.0), block);
			map.insert(block_lbl, id);
		}

		self.node_map = map;
	}

	pub fn parse_ron_data(&mut self) -> () {
		let map = self.node_map.clone();
		let data = std::fs::read(&self.file_path).expect("Incorrect file path");
		let func: Function<Block> = from_bytes(&data).expect("Invalid RON Data");

		//let node_levels = self.assign_node_levels();
		let horizontal_spacing = 150.0;
		let vertical_spacing = 100.0;
		let mut level_counts: HashMap<u32, u32> = HashMap::new();

		for block in func.block_list {
			match &block.edge {
				Control::Unconditional(target) => match target {
					Target::Label(to_label) => {
						let node_from = map.get(&block.label);
						let node_to = map.get(to_label);
						if node_to.is_some() && node_from.is_some() {
							let out_pin: OutPinId = OutPinId {
								node: *node_from.unwrap(),
								output: 0,
							};

							let in_pin: InPinId = InPinId {
								node: *node_to.unwrap(),
								input: 0,
							};
							self.snarl.connect(out_pin, in_pin);
						}
					}

					_ => {}
				},
				Control::Condition(condition, true_target, false_target) => {
					match true_target {
						Target::Label(to_label) => {
							let node_from = map.get(&block.label);
							let node_to = map.get(to_label);
							if node_to.is_some() && node_from.is_some() {
								let out_pin: OutPinId = OutPinId {
									node: *node_from.unwrap(),
									output: 0,
								};

								let in_pin: InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0, // true target is the first pin
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
								let out_pin: OutPinId = OutPinId {
									node: *node_from.unwrap(),
									output: 1, // false target is the 2nd pin
								};

								let in_pin: InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0,
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
								let out_pin: OutPinId = OutPinId {
									node: *node_from.unwrap(),
									output: 0,
								};

								let in_pin: InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0, // true target is the first pin
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
								let out_pin: OutPinId = OutPinId {
									node: *node_from.unwrap(),
									output: 1, // false target is the 2nd pin
								};

								let in_pin: InPinId = InPinId {
									node: *node_to.unwrap(),
									input: 0,
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
		self.ranker_v2();
		/*

			for (node_id, level) in node_levels {
				let x = level as f32 * horizontal_spacing;
				let y = *level_counts.entry(level).or_insert(0) as f32 * vertical_spacing;

				// Set the position of the node in the snarl
				if let Some(node_) = self.snarl.get_node_info_mut(node_id) {
					node_.pos = egui::pos2(x, y);
				}

				// Increment node count in the current level for spacing
				*level_counts.get_mut(&level).unwrap() += 1;
			}
		*/
	}
}

impl Default for EApp {
	fn default() -> Self {
		let snarl: egui_snarl::Snarl<Block> = egui_snarl::Snarl::new();
		let style = egui_snarl::ui::SnarlStyle::new();
		let file_path = String::new();
		let node_map = HashMap::new();

		Self {
			snarl,
			snarl_ui_id: None,
			style,
			file_path,
			node_map,
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

fn ui_mode(file_path: String) -> Result<()> {
	let options = eframe::NativeOptions::default();
	eframe::run_native(
		"LAU | dispatch fork",
		options,
		Box::new(|cc| {
			let mut app = EApp::new(cc);
			app.set_file(file_path);
			app.populate_map();
			let ret = Box::new(app);
			return Ok(ret);
		}), // Cast EApp to Box<dyn App>
	)
	.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())) // Convert eframe::Error to std::io::Error
}

/*
 *
 * Devirtualizer Module (anti vsecure)
 *
 *
 */

fn optimize_jmp(map: &mut HashMap<u32, Block>, visited: &mut HashSet<u32>, node_id: u32) {
	if !visited.insert(node_id) {
		return; // already visited
	}

	if let Some(current_blk) = map.get_mut(&node_id) {
		// the current block should be optimized ?
		for target in current_blk.get_target_labels() {
			if let Some(target_blk) = map.get(&target) {
				if let Some(target_id) = target_blk.get_target_labels().get(0) {
					if target_blk.body.is_empty() && target_blk.is_unconditionnal() {
						// edit the target ffrrr
						println!(
							"fake jmp from {} to {} to {}",
							node_id, target_blk.label, target_id
						);
						
					}
					optimize_jmp(map, visited, *target_id);
				}
			}
		}
	}
}

fn fixup_code_v1(data: &[u8]) -> () {
	// parse data from bytes
	let func_data: Function<Block> = from_bytes(data).expect("Invalid RON data");

	// we need to start from node root and process until the rest of the program from target to
	// target
	// we want to map labels to blocks for faster access !
	let mut block_map: HashMap<u32, Block> = HashMap::new();

	for block in func_data.block_list {
		block_map.insert(block.label, block);
	}

	let r: u32 = 0;
	println!(
		"{} total with first {}",
		block_map.len(),
		block_map[&r].label
	);

	let mut visited = HashSet::new();
	optimize_jmp(&mut block_map, &mut visited, 0);
}

/*
 *
 *
 *
 * MAIN FUNCTION
 *
 *
 */
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
			"-v" | "--devirt" => {
				let name = iter.next().expect("File name expected !");
				let data = std::fs::read(name)?;
				fixup_code_v1(&data);
			}
			opt => {
				panic!("unknown option `{}`", opt);
			}
		}
	}

	Ok(())
}
