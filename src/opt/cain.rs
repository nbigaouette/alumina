use opt::*;
use graph::*;
use vec_math::{VecMath, VecMathMut, VecMathMove};

use supplier::Supplier;
use std::f32;
use std::usize;


pub struct CainBuilder<'a>{
	graph: &'a mut Graph,
	initial_learning_rate: f32,
	initial_subbatch_size: f32,
	config: CainConfig,
}

impl<'a> CainBuilder<'a> {

	pub fn num_subbatches(mut self, val: usize) -> Self{
		self.config.num_subbatches = (val as f32).max(2.0);
		self
	}

	pub fn momentum(mut self, val: f32) -> Self{
		self.config.momentum = val;
		self
	}

	pub fn aggression(mut self, val: f32) -> Self{
		self.config.aggression = val;
		self
	}

	/// target relative std err of the momentum vector
	pub fn target_err(mut self, val: f32) -> Self{
		self.config.target_err = val;
		self
	}

	pub fn subbatch_increase_damping(mut self, val: f32) -> Self{
		self.config.subbatch_increase_damping = val;
		self
	}

	pub fn subbatch_decrease_damping(mut self, val: f32) -> Self{
		self.config.subbatch_increase_damping = val;
		self
	}

	pub fn rate_adapt_coefficient(mut self, val: f32) -> Self{
		self.config.rate_adapt_coefficient = val;
		self
	}

	pub fn max_eval_batch_size(mut self, val: usize) -> Self{
		self.config.max_eval_batch_size = val;
		self
	}

	pub fn min_subbatch_size(mut self, val: usize) -> Self{
		self.config.min_subbatch_size = val;
		if val as f32 > self.initial_subbatch_size {
			self.initial_subbatch_size = val as f32;
		}
		self
	}

	pub fn initial_learning_rate(mut self, val: f32) -> Self{
		self.initial_learning_rate = val;
		self
	}

	pub fn initial_subbatch_size(mut self, val: f32) -> Self{
		self.initial_subbatch_size = val;
		self
	}

	pub fn finish(mut self) -> Cain<'a>{
		let num_params = self.graph.num_params();
		Cain{
			graph: self.graph,
			config: self.config.clone(),

			eval_count: 0,
			step_count: 0,
			
			curvature_est: vec![0.0; num_params],
			learning_rate: self.initial_learning_rate,
			batch_size: self.initial_subbatch_size,

			momentum_derivs: vec![0.0; num_params],
			prev_derivs: vec![0.0; num_params],
			step_callback: vec![],
		}
	}
}

/// A struct to hold variables that dont change after construction
#[derive(Clone)]
struct CainConfig{
	num_subbatches: f32,
	momentum: f32,
	aggression: f32,
	target_err: f32,
	subbatch_increase_damping: f32,
	subbatch_decrease_damping: f32,
	rate_adapt_coefficient: f32,
	max_eval_batch_size: usize,
	min_subbatch_size: usize,
}

/// Cosine Adapted Something Something, a first order optimiser based on ADAM, but with adaptive batch size and step size.
/// Step length is adapted based on the cosine of the derivative vectors of subsequent steps.
/// Each step consists of several sub-batchs ()
/// Nil convergence guarantees.
pub struct Cain<'a>{
	graph: &'a mut Graph,
	config: CainConfig,

	eval_count: u64,
	step_count: u64,
	
	curvature_est: Vec<f32>,
	learning_rate: f32,
	batch_size: f32,

	momentum_derivs: Vec<f32>,
	prev_derivs: Vec<f32>,
	step_callback: Vec<Box<FnMut(CallbackData)->CallbackSignal>>,
}

impl <'a> Cain<'a> {
	pub fn new <'b>(graph: &'b mut Graph) -> CainBuilder<'b>{
		CainBuilder{
			graph: graph,
			initial_learning_rate: 1e-4,
			initial_subbatch_size: 2.0,
			config: CainConfig{
				num_subbatches: 8.0,
				momentum: 0.9,
				aggression: 0.75,
				target_err: 0.75,
				subbatch_increase_damping: 0.15,
				subbatch_decrease_damping: 0.15,
				rate_adapt_coefficient: 1.05,
				max_eval_batch_size: usize::MAX,
				min_subbatch_size: 1,
			}
		}
	}
	

	// pub fn new(graph: &'a mut Graph) -> Cain<'a>{
	// 	let num_params = graph.num_params();
	// 	Cain{
	// 		config: CainConfig,
	// 		eval_count: 0,
	// 		step_count: 0,
	// 		graph: graph,
	// 		curvature_est: vec![0.0; num_params],
	// 		rate: 0.001,
	// 		batch_size: 2.0,
	// 		min_batch_size: 1,
	// 		averaged_derivs: vec![0.0; num_params],
	// 		prev_derivs: vec![0.0; num_params],
	// 		step_callback: vec![],
	// 	}
	// }
	
	// pub fn set_min_batch_size(&mut self, min: usize){
	// 	self.min_batch_size = min;
	// 	self.batch_size = min as f32;
	// }
	
	// pub fn reset_eval_steps(&mut self){
	// 	self.eval_count = 0;
	// 	self.step_count = 0;
	// }

	/// Returns error and error derivatives
	fn part_step(&mut self, training_set: &mut Supplier, params: &[f32], batch_size: u64) -> (f32, Vec<f32>){

			let (input, training_input) = training_set.next_n(batch_size as usize);
			let (mut err, mut param_derivs, _data) = self.graph.backprop(batch_size as usize, input, training_input, &params);
			
			err /= batch_size as f32;
			param_derivs.scale_mut(1.0/batch_size as f32);
			
			self.eval_count += batch_size;
			(err, param_derivs)
			
	}

	/// mutably updates batch_size and returns relative error measure
	fn update_batch_size(&mut self, mean: &[f32], results: &[(f32, Vec<f32>)]) -> f32{


		//-- Hold one out vector variance
		let num_subbatches = self.config.num_subbatches;
		let rel_var = results.iter()
			.fold(0.0f32, |sum, &(_, ref derivs)| {
				// let f = num_subbatches/(num_subbatches-1.0);
				// // hold-one-out mean
				// let hoo_mean: Vec<f32> = mean.iter().zip(derivs).map(|(m,d)| (m - d/num_subbatches) * f ).collect();
				// let diff = derivs.add_scaled(&hoo_mean, -1.0);
				// sum + diff.dot(&diff) / (hoo_mean.dot(&hoo_mean) * num_subbatches)

				let mut diff_dot = 0.0;
				let mut mean_dot = 0.0;
				for (m, d) in mean.iter().zip(derivs){
					let h = (m - d/num_subbatches) * num_subbatches/(num_subbatches-1.0);
					let diff = d-h;
					diff_dot += diff*diff;
					mean_dot += h*h;
				}
				sum + diff_dot/(mean_dot * num_subbatches)
			});

		// target = 1.0 is equivalent to random (orthogonal) unit length vectors on each sample
		let rel_err = (rel_var/num_subbatches).sqrt()/self.config.target_err;




		// //-- Variance when projected onto hold one out mean vector
		// let num_subbatches = self.config.num_subbatches;
		// let projections: Vec<f32> = results.iter()
		// 	.map(|&(_, ref derivs)|{
		// 		let f = num_subbatches/(num_subbatches-1.0);
		// 		// hold-one-out mean
		// 		let hoo_mean: Vec<f32> = mean.iter().zip(derivs).map(|(m,d)| (m - d/num_subbatches) * f ).collect();
		// 		derivs.dot(&hoo_mean)/hoo_mean.norm2()
		// 	}).collect();

		// let mean = projections.iter().sum::<f32>()/num_subbatches;


		// // let mad = projections.iter().fold(0.0, |sum, proj| sum + (proj-mean).abs())/(num_subbatches-1.0);
		// // let rel_err = (1.0-mean*self.config.target_err/(mad/num_subbatches.sqrt())).exp();

		// // true relative error is var.sqrt()/mean/target_err, however this blows up as mean gets small occasionally
		// // instead a stable approximation is used with the same value and gradient at the target error
		// // this technique works for positive target_err
		// let var = projections.iter().fold(0.0, |sum, proj| sum + (proj-mean)*(proj-mean))/(num_subbatches-1.0);
		// let rel_err = (1.0-mean*self.config.target_err/(var/num_subbatches).sqrt()).exp();



		
		
		// Adapt batch size based on derivative relative err vs target relative variance
		let rel_err = rel_err.max(0.125).min(1000.0);
		self.batch_size *= if rel_err > 1.0 {
				rel_err.powf(self.config.subbatch_increase_damping) // increase batch size 0.075
			} else {
				rel_err.powf(self.config.subbatch_decrease_damping) // decrease batch size
			};
		self.batch_size = self.batch_size.max(self.config.min_subbatch_size as f32);
		rel_err
	}

	fn update_curvature(&mut self, mean: &[f32]){ //, results: &[(f32, Vec<f32>)]
				
		//let curv_decay = self.config.momentum.powf(0.09539).max(0.9);
		let curv_decay = self.config.momentum.powf(1.0/4.0).max(0.9);
		self.curvature_est.scale_mut(curv_decay);			
		
		//-- Also incorperate randomness
		// for &(_, ref derivs) in results.iter() {
		// 	let var = derivs.add_scaled(&self.momentum_derivs, -1.0);
		// 	self.curvature_est.add_scaled_mut(&var.elementwise_mul(&var), (1.0 - curv_decay)/self.config.num_subbatches);	
		// 	//self.curvature_est.add_scaled_mut(&derivs.elementwise_mul(&derivs), (1.0 - curv_decay)/NUM_BINS as f32);				
		// }

		let n = mean.len();
		let curv = &mut self.curvature_est[0..n];
		let momen = &self.momentum_derivs[0..n];
		let mean = &mean[0..n];
		for i in 0..n{
			let diff = mean[i] - momen[i];
			curv[i] += diff*diff*(1.0-curv_decay)
		}

		// let var = mean.add_scaled(&self.momentum_derivs, -1.0);
		// self.curvature_est.add_scaled_mut(&var.elementwise_mul(&var), (1.0 - curv_decay) as f32);



	}
}

impl<'a> Optimiser<'a> for Cain<'a>{

	fn add_boxed_step_callback(&mut self, func: Box<FnMut(CallbackData)->CallbackSignal>){ // err, step, evaluations, graph, params
		self.step_callback.push(func);
	}

	fn get_graph(&mut self) -> &mut Graph{
		&mut self.graph
	}

	fn optimise_from(&mut self, training_set: &mut Supplier,  mut params: Vec<f32>) -> Vec<f32>{

		'outer: loop {
			let (err, new_params) = self.step(training_set, params);
			params = new_params;

			
			for func in self.step_callback.iter_mut(){
				let data = CallbackData{err: err, step_count: self.step_count, eval_count: self.eval_count, graph: &self.graph, params: &params};
				match func(data){
					CallbackSignal::Stop => {break 'outer},
					CallbackSignal::Continue =>{},
				}
			}
		}
		
		params
	}


	
	fn step(&mut self, training_set: &mut Supplier, params: Vec<f32>) -> (f32, Vec<f32>){

		// Take multiple measurements of error and gradient, then find L2 variance in derivative vectors
		
		let batch_ceil = self.batch_size.floor() as u64;
		let results = (0..self.config.num_subbatches as usize).map(|_| self.part_step(training_set, &params, batch_ceil)).collect::<Vec<_>>();
		
		let err: f32 = results.iter().fold(0.0f32, |acc, &(err, _)| acc + err)/self.config.num_subbatches;
		let mean: Vec<f32> = results.iter().fold(vec![0.0f32;params.len()], |acc, &(_, ref derivs)| acc.add_move(&derivs)).scale_move(1.0/self.config.num_subbatches);

		let rel_err = self.update_batch_size(&mean, &results);
	// {
	// 	let avg_norm_sqr = self.momentum_derivs.dot(&self.momentum_derivs);
	// 	let mean_dot = mean.dot(&self.momentum_derivs)/avg_norm_sqr;
	// 	let prev_dot = self.prev_derivs.dot(&self.momentum_derivs)/avg_norm_sqr;


	// 	let mean_reg = mean.add_scaled(&self.momentum_derivs, -mean_dot);
	// 	let prev_reg = self.prev_derivs.add_scaled(&self.momentum_derivs, -prev_dot);
	// 	let mut sim = mean_reg.cos_similarity(&prev_reg);
	// 	//if sim > 0.0 {(sim = sim/2.0);}

	// 	let rate: f32 = 1.05;
	// 	let mut change = rate.powf(sim);
	// 	//if change > 1.0 {change = change.powf(0.25);}
	// 	self.config.momentum = self.config.momentum.powf(change).max(0.5).min(0.9999);
	// 	println!("sim: {} momen: {} chan: {}", sim, self.config.momentum, change);
	// }


		self.update_curvature(&mean);

		//let sim = mean.add_scaled(&self.prev_derivs, self.config.aggression).cos_similarity(&self.prev_derivs);

		//let sim = mean.add_scaled(&self.momentum_derivs, self.config.aggression).cos_similarity(&self.momentum_derivs);
		// let sim = (mean.add_scaled(&self.momentum_derivs, self.config.aggression).dot(&self.momentum_derivs)/self.momentum_derivs.dot(&self.momentum_derivs)).max(-4.0).min(2.0); // clipping prevents unexpected noise results from blowing up step size adaption.
		let sim = if self.step_count == 0 {
			0.0
		} else {
			(self.config.aggression + mean.dot(&self.momentum_derivs)/self.momentum_derivs.dot(&self.momentum_derivs)).max(-8.0).min(4.0)
		};

		let new_rate = self.learning_rate*self.config.rate_adapt_coefficient.powf(sim);//+self.config.aggression
		

		self.momentum_derivs.scale_mut(self.config.momentum);
		self.momentum_derivs.add_scaled_mut(&mean, 1.0 - self.config.momentum);

		// let corrected_curvature = self.curvature_est.scale(1.0/(1.0 - curv_decay.powi(self.step_count as i32 + 1)));
		let curv_decay = self.config.momentum.powf(1.0/4.0).max(0.9);
		let curvature_step_correction = if self.step_count < 1_000_000{1.0/(1.0 - curv_decay.powi(self.step_count as i32 + 1))} else {1.0};
		let momentum_step_correction = 1.0;//if self.step_count < 1_000_000{1.0/(1.0 - self.config.momentum.powi(self.step_count as i32 + 1))} else {1.0};
		let cond_derivs: Vec<f32> = self.momentum_derivs.iter().zip(&self.curvature_est).map(|(m,c)| m*momentum_step_correction/((c*curvature_step_correction).sqrt() + 1e-8)).collect();

		
		
		let change = cond_derivs.scale_move(-new_rate);



		// print progress (this should be moved to a callback lambda)
		if self.step_count == 0 {println!("");println!("count\terr\trel_err\tbatchSize\tcos_sim\trate\tmovement");}
		println!("{}\t{}\t{:.4}\t{}x{}\t{:.4}\t{:.4e}\t{:.4e}", training_set.samples_taken(), err, rel_err, self.config.num_subbatches, self.batch_size.floor(), sim, new_rate, change.norm2());


		let new_params = change.add_move(&params);

		self.learning_rate = new_rate;
		self.step_count += 1;
		self.prev_derivs = mean;
		(err, new_params)
	}

		
}