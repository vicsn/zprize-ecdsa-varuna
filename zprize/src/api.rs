// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use rand::rngs::OsRng;
use snarkvm_algorithms::{
    crypto_hash::PoseidonSponge,
    polycommit::kzg10::UniversalParams,
    snark::varuna::{self, AHPForR1CS, CircuitProvingKey, CircuitVerifyingKey, VarunaHidingMode},
    traits::SNARK,
};
use snarkvm_circuit::{
    environment::{Assignment, Circuit},
    Environment as _,
};
use snarkvm_circuit_environment::{Inject, Mode};
use snarkvm_console::network::Testnet3 as Network;
use snarkvm_console_network::Network as _;
use snarkvm_curves::bls12_377::{Bls12_377, Fq, Fr};
use std::collections::BTreeMap;

use crate::circuit;
use crate::console;

//
// Aliases
// =======
//

type FS = PoseidonSponge<Fq, 2, 1>;
type VarunaInst = varuna::VarunaSNARK<Bls12_377, FS, VarunaHidingMode>;

//
// Functions
// =========
//

/// Our circuit synthesizer.
pub fn run_circuit(
    public_key: console::ECDSAPublicKey,
    signature: console::ECDSASignature,
    msg: Vec<u8>,
) -> Assignment<Fr> {
    // reset circuit writer
    Circuit::reset();

    // sample msg and witness as public input
    let msg = circuit::Message::new(Mode::Public, msg.clone());

    // sample pubkey and sig and witness as public input
    let public_key = circuit::ECDSAPublicKey::new(Mode::Public, public_key);
    let signature = circuit::ECDSASignature::new(Mode::Public, signature);

    // run circuit
    circuit::verify_one(public_key, signature, msg);

    // return circuit
    Circuit::eject_assignment_and_reset()
}

/// Setup the parameters.
pub fn setup(
    num_constraints: usize,
    num_variables: usize,
    num_non_zero: usize,
) -> UniversalParams<Bls12_377> {
    // Note: you can change this to increase the size of the circuit.
    // Of course, the higher these values, the slower the prover...
    let max_degree = AHPForR1CS::<Fr, VarunaHidingMode>::max_degree(
        num_constraints,
        num_variables,
        num_non_zero,
    )
    .unwrap();
    VarunaInst::universal_setup(max_degree).unwrap()
}

/// Compile the circuit.
pub fn compile(
    urs: &UniversalParams<Bls12_377>,
    msg_len: usize,
) -> (
    CircuitProvingKey<Bls12_377, VarunaHidingMode>,
    CircuitVerifyingKey<Bls12_377>,
) {
    let msg = console::sample_msg(msg_len);
    let (public_key, signature) = console::sample_pubkey_sig(&msg);
    let circuit = run_circuit(public_key, signature, msg);
    println!("num constraints: {}", circuit.num_constraints());
    println!("num public: {}", circuit.num_public());
    println!("num private: {}", circuit.num_private());
    println!("num non-zeros: {:?}", circuit.num_nonzeros());
    VarunaInst::circuit_setup(&urs, &circuit).unwrap()
}

/// Run and prove the circuit.
pub fn prove(
    urs: &UniversalParams<Bls12_377>,
    pk: &CircuitProvingKey<Bls12_377, VarunaHidingMode>,
    public_key: console::ECDSAPublicKey,
    msg: Vec<u8>,
    signature: console::ECDSASignature,
) -> varuna::Proof<Bls12_377> {
    // run circuit
    let circuit = run_circuit(public_key, signature, msg);

    // Prepare the instances.
    let mut instances = BTreeMap::new();
    let vec_of_circuits = vec![circuit];
    instances.insert(pk, &vec_of_circuits[..]);

    // Compute the proof.
    let rng = &mut OsRng::default();
    let universal_prover = urs.to_universal_prover().unwrap();
    let fiat_shamir = Network::varuna_fs_parameters();

    // Note: this could be optimized to prove several instances instead of just one ;)
    let res = VarunaInst::prove_batch(&universal_prover, fiat_shamir, &instances, rng).unwrap();
    res
}

/// Verify a proof.
pub fn verify_proof(
    urs: &UniversalParams<Bls12_377>,
    vk: &CircuitVerifyingKey<Bls12_377>,
    public_key: console::ECDSAPublicKey,
    msg: Vec<u8>,
    signature: console::ECDSASignature,
    proof: &varuna::Proof<Bls12_377>,
) {
    // Note: this is a hacky way of formatting public inputs,
    // we shouldn't have to run the circuit to do that.
    let circuit = run_circuit(public_key, signature, msg);
    let mut inputs = vec![];
    for (_, input) in circuit.public_inputs() {
        inputs.push(*input);
    }
    let vec_of_inputs = vec![inputs];

    // verify
    let mut keys_to_inputs = BTreeMap::new();
    keys_to_inputs.insert(vk, &vec_of_inputs[..]);
    let universal_verifier = urs.to_universal_verifier().unwrap();
    let fiat_shamir = Network::varuna_fs_parameters();

    // Note: same comment here, verify_batch could verify several proofs instead of one ;)
    VarunaInst::verify_batch(&universal_verifier, fiat_shamir, &keys_to_inputs, proof).unwrap();
}
