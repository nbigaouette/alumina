use new::graph::{GraphDef, NodeID, OpID, Result};
use new::ops::Op;
use new::ops::activ::elementwise::{ActivationFunc, ElementwiseInstance, elementwise_build};

#[derive(Clone, Debug)] 
pub struct SrgbToLinearFunc{}

impl ActivationFunc for SrgbToLinearFunc {
	fn value(&self, input: f32) -> f32{
		if input <= 0.0404482362771082{
			input/12.92
		} else {
			0.001522305 + 0.012475774*input + 0.662456816212772*input*input + 0.32679397543773*input*input*input
		}
	}

	fn gradient(&self, input: f32, output_grad: f32) -> f32{
		if input <= 0.0404482362771082{
			 output_grad/12.92
		} else {
			 output_grad*(0.012475774 + 2.0*0.662456816212772*input + 3.0*0.32679397543773*input*input)
		}
	}

	fn backprop_requires_input_value() -> bool {true}
}

#[derive(Clone, Debug)] 
pub struct SrgbToLinear {
	output: NodeID,
	input: NodeID,
	name: Option<String>,
}

impl SrgbToLinear {
	pub fn new(input: &NodeID, output: &NodeID) -> Self {
		SrgbToLinear {
			input: input.clone(),
			output: output.clone(),
			name: None,
		}
	}
}

impl Op for SrgbToLinear {
	type InstanceType = ElementwiseInstance<SrgbToLinearFunc>;

	fn type_name(&self) -> &'static str {
		"SrgbToLinear"
	}

	fn name<T: Into<String>>(mut self, name: T) -> Self{
		self.name = Some(name.into());
		self
	}

	fn build(self, graph: &mut GraphDef, _op_id: &OpID) -> Result<Self::InstanceType> {
		elementwise_build(graph, &self, &self.name, &self.input, &self.output, SrgbToLinearFunc{})
	}
}




#[derive(Clone, Debug)] 
pub struct LinearToSrgbFunc{}

impl ActivationFunc for LinearToSrgbFunc {
	fn value(&self, input: f32) -> f32{
		if input <= 0.00313066844250063{
			input*12.92
		} else {
			let s1 = input.sqrt();
			let s2 = s1.sqrt();
			-0.074312538 + 0.852548197*s1 + 0.284336309*s2 - 0.063628643*input
		}
	}

	fn gradient(&self, input: f32, output_grad: f32) -> f32{
		if input <= 0.00313066844250063{
			output_grad*12.92
		} else {
			let s1 = input.sqrt();
			let s2 = s1.sqrt();
			output_grad*(0.5*0.852548197/s1+ 0.25*0.284336309/(s1*s2) - 0.063628643)
		}
	}

	fn backprop_requires_input_value() -> bool {true}
}


#[derive(Clone, Debug)] 
pub struct LinearToSrgb {
	output: NodeID,
	input: NodeID,
	name: Option<String>,
}

impl LinearToSrgb {
	pub fn new(input: &NodeID, output: &NodeID) -> Self {
		LinearToSrgb {
			input: input.clone(),
			output: output.clone(),
			name: None,
		}
	}
}

impl Op for LinearToSrgb {
	type InstanceType = ElementwiseInstance<LinearToSrgbFunc>;

	fn type_name(&self) -> &'static str {
		"LinearToSrgb"
	}

	fn name<T: Into<String>>(mut self, name: T) -> Self{
		self.name = Some(name.into());
		self
	}

	fn build(self, graph: &mut GraphDef, _op_id: &OpID) -> Result<Self::InstanceType> {
		elementwise_build(graph, &self, &self.name, &self.input, &self.output, LinearToSrgbFunc{})
	}
}




#[test]
fn test_srgb_to_linear_backprop(){
	_srgb_to_linear_backprop().unwrap();
}

fn _srgb_to_linear_backprop() -> Result<()>{
	use new::graph::GraphDef;
	use new::ops::numeric_check::numeric_test;
	use new::ops::loss::mse::Mse;
	use ordermap::OrderMap;

	let mut g = GraphDef::new();

	let node1 = g.new_node(shape![7, 5, 16], "input", tag![])?;
	let node2 = g.new_node(shape![7, 5, 16], "output", tag![])?;
	let node3 = g.new_node(shape![7, 5, 16], "target", tag![])?;


	let _o1 = g.new_op(SrgbToLinear::new(&node1, &node2), tag![])?;
	let _o2 = g.new_op(Mse::new(&node2, &node3), tag![])?;

	let iters = 100;
	let failures = 1;
	let tolerance = 0.002;
	let step_size = 1E-2;
	let default_variance = 1.0;
	numeric_test(iters, failures, tolerance, &g, step_size, default_variance, &mut OrderMap::new())?;

	Ok(())
}

#[test]
fn test_linear_to_srgb_backprop(){
	_linear_to_srgb_backprop().unwrap();
}

fn _linear_to_srgb_backprop() -> Result<()>{
	use new::graph::GraphDef;
	use new::ops::numeric_check::numeric_test;
	use new::ops::loss::mse::Mse;
	use ordermap::OrderMap;

	let mut g = GraphDef::new();

	let node1 = g.new_node(shape![7, 5, 16], "input", tag![])?;
	let node2 = g.new_node(shape![7, 5, 16], "output", tag![])?;
	let node3 = g.new_node(shape![7, 5, 16], "target", tag![])?;


	let _o1 = g.new_op(LinearToSrgb::new(&node1, &node2), tag![])?;
	let _o2 = g.new_op(Mse::new(&node2, &node3), tag![])?;

	let iters = 100;
	let failures = 1;
	let tolerance = 0.005; // why is the accuracy so much worse? cancellation at the high end?
	let step_size = 1E-2;
	let default_variance = 0.5;
	numeric_test(iters, failures, tolerance, &g, step_size, default_variance, &mut OrderMap::new())?;

	Ok(())
}