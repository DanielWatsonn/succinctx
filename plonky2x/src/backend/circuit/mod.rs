pub mod input;
pub mod mock;
pub mod output;
pub mod serialization;
pub mod witness;

use std::fs;

use itertools::Itertools;
use plonky2::field::types::PrimeField64;
use plonky2::iop::witness::PartialWitness;
use plonky2::plonk::circuit_data::CircuitData;
use plonky2::plonk::config::{AlgebraicHasher, GenericConfig, GenericHashOut};
use plonky2::plonk::proof::ProofWithPublicInputs;
use plonky2::util::serialization::{
    Buffer, GateSerializer, IoResult, Read, WitnessGeneratorSerializer, Write,
};

use self::input::PublicInput;
use self::output::PublicOutput;
use self::serialization::{GateRegistry, WitnessGeneratorRegistry};
use super::config::PlonkParameters;
use crate::frontend::builder::io::{BytesIO, ElementsIO};
use crate::frontend::builder::CircuitIO;
use crate::prelude::{ByteVariable, CircuitVariable, Variable};
use crate::utils::hex;

/// A compiled circuit.
///
/// It can compute a function in the form f(publicInputs, privateInputs) = publicOutputs.
#[derive(Debug)]
pub struct Circuit<L: PlonkParameters<D>, const D: usize> {
    pub data: CircuitData<L::Field, L::Config, D>,
    pub io: CircuitIO<D>,
}

impl<L: PlonkParameters<D>, const D: usize> Circuit<L, D> {
    /// Returns an input instance for the circuit.
    pub fn input(&self) -> PublicInput<L, D> {
        PublicInput::new(&self.io)
    }

    /// Generates a proof for the circuit. The proof can be verified using `verify`.
    pub fn prove(
        &self,
        input: &PublicInput<L, D>,
    ) -> (
        ProofWithPublicInputs<L::Field, L::Config, D>,
        PublicOutput<L, D>,
    ) {
        let mut pw = PartialWitness::new();
        self.io.set_witness(&mut pw, input);
        let proof_with_pis = self.data.prove(pw).unwrap();
        let output = PublicOutput::from_proof_with_pis(&self.io, &proof_with_pis);
        (proof_with_pis, output)
    }

    /// Verifies a proof for the circuit.
    pub fn verify(
        &self,
        proof: &ProofWithPublicInputs<L::Field, L::Config, D>,
        input: &PublicInput<L, D>,
        output: &PublicOutput<L, D>,
    ) {
        let expected_input = PublicInput::<L, D>::from_proof_with_pis(&self.io, proof);
        let expected_output = PublicOutput::<L, D>::from_proof_with_pis(&self.io, proof);
        assert_eq!(input, &expected_input);
        assert_eq!(output, &expected_output);
        self.data.verify(proof.clone()).unwrap();
    }

    pub fn id(&self) -> String {
        let circuit_digest = hex!(self
            .data
            .verifier_only
            .circuit_digest
            .to_vec()
            .iter()
            .flat_map(|e| e.to_canonical_u64().to_be_bytes())
            .collect::<Vec<u8>>());
        circuit_digest[0..22].to_string()
    }

    pub fn serialize(
        &self,
        gate_serializer: &impl GateSerializer<L::Field, D>,
        generator_serializer: &impl WitnessGeneratorSerializer<L::Field, D>,
    ) -> IoResult<Vec<u8>> {
        // Setup buffer.
        let mut buffer = Vec::new();
        let circuit_bytes = self.data.to_bytes(gate_serializer, generator_serializer)?;
        buffer.write_usize(circuit_bytes.len())?;
        buffer.write_all(&circuit_bytes)?;
        match &self.io {
            CircuitIO::Bytes(io) => {
                buffer.write_usize(0)?;
                buffer.write_target_vec(
                    io.input
                        .iter()
                        .flat_map(|b| b.targets())
                        .collect_vec()
                        .as_slice(),
                )?;
                buffer.write_target_vec(
                    io.output
                        .iter()
                        .flat_map(|b| b.targets())
                        .collect_vec()
                        .as_slice(),
                )?;
            }
            CircuitIO::Elements(io) => {
                buffer.write_usize(1)?;
                buffer.write_target_vec(io.input.iter().map(|v| v.0).collect_vec().as_slice())?;
                buffer.write_target_vec(io.output.iter().map(|v| v.0).collect_vec().as_slice())?;
            }
            CircuitIO::None() => {
                buffer.write_usize(2)?;
            }
            _ => panic!("unsupported io type"),
        }

        Ok(buffer)
    }

    pub fn deserialize(
        buffer: &[u8],
        gate_serializer: &impl GateSerializer<L::Field, D>,
        generator_serializer: &impl WitnessGeneratorSerializer<L::Field, D>,
    ) -> IoResult<Self> {
        // Setup buffer.
        let mut buffer = Buffer::new(buffer);

        // Read circuit data from bytes.
        let circuit_bytes_len = buffer.read_usize()?;
        let mut circuit_bytes = vec![0u8; circuit_bytes_len];
        buffer.read_exact(circuit_bytes.as_mut_slice())?;
        let data = CircuitData::<L::Field, L::Config, D>::from_bytes(
            &circuit_bytes,
            gate_serializer,
            generator_serializer,
        )?;

        let mut circuit = Circuit {
            data,
            io: CircuitIO::new(),
        };

        let io_type = buffer.read_usize()?;
        if io_type == 0 {
            let input_targets = buffer.read_target_vec()?;
            let output_targets = buffer.read_target_vec()?;
            let input_bytes = (0..input_targets.len() / 8)
                .map(|i| ByteVariable::from_targets(&input_targets[i * 8..(i + 1) * 8]))
                .collect_vec();
            let output_bytes = (0..output_targets.len() / 8)
                .map(|i| ByteVariable::from_targets(&output_targets[i * 8..(i + 1) * 8]))
                .collect_vec();
            circuit.io = CircuitIO::Bytes(BytesIO {
                input: input_bytes,
                output: output_bytes,
            });
        } else if io_type == 1 {
            let input_targets = buffer.read_target_vec()?;
            let output_targets = buffer.read_target_vec()?;
            circuit.io = CircuitIO::Elements(ElementsIO {
                input: input_targets.into_iter().map(Variable).collect_vec(),
                output: output_targets.into_iter().map(Variable).collect_vec(),
            });
        }

        Ok(circuit)
    }

    pub fn save(
        &self,
        path: &String,
        gate_serializer: &impl GateSerializer<L::Field, D>,
        generator_serializer: &impl WitnessGeneratorSerializer<L::Field, D>,
    ) {
        let bytes = self
            .serialize(gate_serializer, generator_serializer)
            .unwrap();
        fs::write(path, bytes).unwrap();
    }

    pub fn load(
        path: &str,
        gate_serializer: &impl GateSerializer<L::Field, D>,
        generator_serializer: &impl WitnessGeneratorSerializer<L::Field, D>,
    ) -> IoResult<Self> {
        let bytes = fs::read(path).unwrap();
        Self::deserialize(bytes.as_slice(), gate_serializer, generator_serializer)
    }

    pub fn save_to_build_dir(
        &self,
        gate_serializer: &impl GateSerializer<L::Field, D>,
        generator_serializer: &impl WitnessGeneratorSerializer<L::Field, D>,
    ) {
        let path = format!("./build/{}.circuit", self.id());
        self.save(&path, gate_serializer, generator_serializer);
    }

    pub fn load_from_build_dir(
        circuit_id: String,
        gate_serializer: &impl GateSerializer<L::Field, D>,
        generator_serializer: &impl WitnessGeneratorSerializer<L::Field, D>,
    ) -> IoResult<Self> {
        let path = format!("./build/{}.circuit", circuit_id);
        Self::load(&path, gate_serializer, generator_serializer)
    }

    pub fn test_default_serializers(&self)
    where
        <<L as PlonkParameters<D>>::Config as GenericConfig<D>>::Hasher: AlgebraicHasher<L::Field>,
    {
        let gate_serializer = GateRegistry::<L, D>::new();
        let generator_serializer = WitnessGeneratorRegistry::<L, D>::new();
        self.test_serializers(&gate_serializer, &generator_serializer);
    }

    pub fn test_serializers(
        &self,
        gate_serializer: &GateRegistry<L, D>,
        generator_serializer: &WitnessGeneratorRegistry<L, D>,
    ) {
        let serialized_bytes = self
            .serialize(gate_serializer, generator_serializer)
            .unwrap();
        let deserialized_circuit = Self::deserialize(
            serialized_bytes.as_slice(),
            gate_serializer,
            generator_serializer,
        )
        .unwrap();
        assert_eq!(self.data, deserialized_circuit.data);
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use plonky2::field::types::Field;

    use crate::backend::circuit::serialization::{GateRegistry, WitnessGeneratorRegistry};
    use crate::backend::circuit::Circuit;
    use crate::backend::config::DefaultParameters;
    use crate::frontend::builder::CircuitBuilderX;
    use crate::prelude::*;

    type L = DefaultParameters;
    const D: usize = 2;

    #[test]
    fn test_serialize_with_field_io() {
        // Define your circuit.
        let mut builder = CircuitBuilderX::new();
        let a = builder.read::<Variable>();
        let b = builder.read::<Variable>();
        let c = builder.add(a, b);
        builder.write(c);

        // Build your circuit.
        let circuit = builder.build();

        // Write to the circuit input.
        let mut input = circuit.input();
        input.write::<Variable>(GoldilocksField::TWO);
        input.write::<Variable>(GoldilocksField::TWO);

        // Generate a proof.
        let (proof, output) = circuit.prove(&input);

        // Verify proof.
        circuit.verify(&proof, &input, &output);

        // Setup serializers
        let gate_serializer = GateRegistry::<L, D>::new();
        let generator_serializer = WitnessGeneratorRegistry::<L, D>::new();

        // Serialize.
        let bytes = circuit
            .serialize(&gate_serializer, &generator_serializer)
            .unwrap();
        let old_digest = circuit.data.verifier_only.circuit_digest;
        let old_input_variables = circuit.io.input();
        let old_output_variables = circuit.io.output();

        // Deserialize.
        let circuit =
            Circuit::<L, D>::deserialize(&bytes, &gate_serializer, &generator_serializer).unwrap();
        let new_digest = circuit.data.verifier_only.circuit_digest;
        let new_input_variables = circuit.io.input();
        let new_output_variables = circuit.io.output();

        // Perform some sanity checks.
        assert_eq!(old_digest, new_digest);
        assert_eq!(old_input_variables.len(), new_input_variables.len());
        assert_eq!(old_output_variables.len(), new_output_variables.len());
        for i in 0..old_input_variables.len() {
            assert_eq!(old_input_variables[i].0, new_input_variables[i].0);
        }
        for i in 0..old_output_variables.len() {
            assert_eq!(old_output_variables[i].0, new_output_variables[i].0);
        }
    }

    #[test]
    fn test_serialize_with_evm_io() {
        // Define your circuit.
        let mut builder = CircuitBuilderX::new();
        let a = builder.evm_read::<ByteVariable>();
        let b = builder.evm_read::<ByteVariable>();
        let c = builder.xor(a, b);
        builder.evm_write(c);

        // Build your circuit.
        let circuit = builder.build();

        // Write to the circuit input.
        let mut input = circuit.input();
        input.evm_write::<ByteVariable>(0u8);
        input.evm_write::<ByteVariable>(1u8);

        // Generate a proof.
        let (proof, output) = circuit.prove(&input);

        // Verify proof.
        circuit.verify(&proof, &input, &output);

        // Setup serializers
        let gate_serializer = GateRegistry::<L, D>::new();
        let generator_serializer = WitnessGeneratorRegistry::<L, D>::new();

        // Serialize.
        let bytes = circuit
            .serialize(&gate_serializer, &generator_serializer)
            .unwrap();
        let old_digest = circuit.data.verifier_only.circuit_digest;
        let old_input_bytes = circuit.io.input();
        let old_output_bytes = circuit.io.output();

        // Deserialize.
        let circuit =
            Circuit::<L, D>::deserialize(&bytes, &gate_serializer, &generator_serializer).unwrap();
        let new_digest = circuit.data.verifier_only.circuit_digest;
        let new_input_bytes = circuit.io.input();
        let new_output_bytes = circuit.io.output();

        // Perform some sanity checks.
        assert_eq!(old_digest, new_digest);
        assert_eq!(old_input_bytes.len(), new_input_bytes.len());
        assert_eq!(old_output_bytes.len(), new_output_bytes.len());
        for i in 0..old_input_bytes.len() {
            let old_targets = old_input_bytes[i].targets();
            let new_targets = new_input_bytes[i].targets();
            assert_eq!(old_targets.len(), new_targets.len());
            for j in 0..old_targets.len() {
                assert_eq!(old_targets[j], new_targets[j]);
            }
        }
        for i in 0..old_output_bytes.len() {
            let old_targets = old_output_bytes[i].targets();
            let new_targets = new_output_bytes[i].targets();
            assert_eq!(old_targets.len(), new_targets.len());
            for j in 0..old_targets.len() {
                assert_eq!(old_targets[j], new_targets[j]);
            }
        }
    }
}
