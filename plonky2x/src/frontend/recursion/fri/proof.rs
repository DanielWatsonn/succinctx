use plonky2::fri::proof::{
    FriInitialTreeProofTarget, FriProofTarget, FriQueryRoundTarget, FriQueryStepTarget,
};
use plonky2::fri::FriParams;
use plonky2::gadgets::polynomial::PolynomialCoeffsExtTarget;
use plonky2::hash::hash_types::MerkleCapTarget;
use plonky2::iop::ext_target::ExtensionTarget;

use crate::frontend::recursion::extension::ExtensionVariable;
use crate::frontend::recursion::hash::{MerkleCapVariable, MerkleProofVariable};
use crate::frontend::recursion::polynomial::PolynomialCoeffsExtVariable;
use crate::frontend::vars::VariableStream;
use crate::prelude::Variable;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriProofVariable<const D: usize> {
    pub commit_phase_merkle_caps: Vec<MerkleCapVariable>,
    pub query_round_proofs: Vec<FriQueryRoundVriable<D>>,
    pub final_poly: PolynomialCoeffsExtVariable<D>,
    pub pow_witness: Variable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriQueryRoundVriable<const D: usize> {
    pub initial_trees_proof: FriInitialTreeProofVariable,
    pub steps: Vec<FriQueryStepVariable<D>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriInitialTreeProofVariable {
    pub evals_proofs: Vec<(Vec<Variable>, MerkleProofVariable)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FriQueryStepVariable<const D: usize> {
    pub evals: Vec<ExtensionVariable<D>>,
    pub merkle_proof: MerkleProofVariable,
}

impl VariableStream {
    pub fn read_fri_proof<const D: usize>(
        &mut self,
        num_leaves_per_oracle: &[usize],
        params: &FriParams,
    ) -> FriProofVariable<D> {
        let cap_height = params.config.cap_height;
        let num_queries = params.config.num_query_rounds;

        let commit_phase_merkle_caps = (0..params.reduction_arity_bits.len())
            .map(|_| self.read_merkle_cap(cap_height))
            .collect::<Vec<_>>();

        let query_round_proofs = (0..num_queries)
            .map(|_| self.read_fri_query_round(num_leaves_per_oracle, params))
            .collect::<Vec<_>>();

        let final_poly = self.read_poly_coeff_ext(params.final_poly_len());
        let pow_witness = self.read::<Variable>();
        FriProofVariable {
            commit_phase_merkle_caps,
            query_round_proofs,
            final_poly,
            pow_witness,
        }
    }

    pub fn read_poly_coeff_ext<const D: usize>(
        &mut self,
        len: usize,
    ) -> PolynomialCoeffsExtVariable<D> {
        PolynomialCoeffsExtVariable(
            (0..len)
                .map(|_| self.read::<ExtensionVariable<D>>())
                .collect(),
        )
    }

    pub fn read_fri_query_round<const D: usize>(
        &mut self,
        num_leaves_per_oracle: &[usize],
        params: &FriParams,
    ) -> FriQueryRoundVriable<D> {
        let cap_height = params.config.cap_height;
        assert!(params.lde_bits() >= cap_height);
        let mut merkle_proof_len = params.lde_bits() - cap_height;

        let initial_trees_proof =
            self.read_virtual_fri_initial_trees_proof(num_leaves_per_oracle, merkle_proof_len);

        let mut steps = Vec::with_capacity(params.reduction_arity_bits.len());
        for &arity_bits in &params.reduction_arity_bits {
            assert!(merkle_proof_len >= arity_bits);
            merkle_proof_len -= arity_bits;
            steps.push(self.read_virtual_fri_query_step(arity_bits, merkle_proof_len));
        }

        FriQueryRoundVriable {
            initial_trees_proof,
            steps,
        }
    }

    fn read_virtual_fri_initial_trees_proof(
        &mut self,
        num_leaves_per_oracle: &[usize],
        initial_merkle_proof_len: usize,
    ) -> FriInitialTreeProofVariable {
        let evals_proofs = num_leaves_per_oracle
            .iter()
            .map(|&num_oracle_leaves| {
                let leaves = self.read_exact(num_oracle_leaves).to_vec();
                let merkle_proof = self.read_merkle_proof(initial_merkle_proof_len);
                (leaves, merkle_proof)
            })
            .collect();
        FriInitialTreeProofVariable { evals_proofs }
    }

    fn read_virtual_fri_query_step<const D: usize>(
        &mut self,
        arity_bits: usize,
        merkle_proof_len: usize,
    ) -> FriQueryStepVariable<D> {
        FriQueryStepVariable {
            evals: self.read_vec::<ExtensionVariable<D>>(1 << arity_bits),
            merkle_proof: self.read_merkle_proof(merkle_proof_len),
        }
    }
}

impl From<FriInitialTreeProofTarget> for FriInitialTreeProofVariable {
    fn from(value: FriInitialTreeProofTarget) -> Self {
        Self {
            evals_proofs: value
                .evals_proofs
                .into_iter()
                .map(|(evals, merkle_proof)| {
                    (
                        evals.into_iter().map(Variable).collect(),
                        merkle_proof.into(),
                    )
                })
                .collect(),
        }
    }
}

impl From<FriInitialTreeProofVariable> for FriInitialTreeProofTarget {
    fn from(value: FriInitialTreeProofVariable) -> Self {
        Self {
            evals_proofs: value
                .evals_proofs
                .into_iter()
                .map(|(evals, merkle_proof)| {
                    (
                        evals.into_iter().map(|v| v.0).collect(),
                        merkle_proof.into(),
                    )
                })
                .collect(),
        }
    }
}

impl<const D: usize> From<FriQueryStepVariable<D>> for FriQueryStepTarget<D> {
    fn from(value: FriQueryStepVariable<D>) -> Self {
        Self {
            evals: value.evals.into_iter().map(ExtensionTarget::from).collect(),
            merkle_proof: value.merkle_proof.into(),
        }
    }
}

impl<const D: usize> From<FriQueryStepTarget<D>> for FriQueryStepVariable<D> {
    fn from(value: FriQueryStepTarget<D>) -> Self {
        Self {
            evals: value
                .evals
                .into_iter()
                .map(ExtensionVariable::from)
                .collect(),
            merkle_proof: value.merkle_proof.into(),
        }
    }
}

impl<const D: usize> From<FriQueryRoundTarget<D>> for FriQueryRoundVriable<D> {
    fn from(value: FriQueryRoundTarget<D>) -> Self {
        Self {
            initial_trees_proof: value.initial_trees_proof.into(),
            steps: value
                .steps
                .into_iter()
                .map(FriQueryStepVariable::from)
                .collect(),
        }
    }
}

impl<const D: usize> From<FriQueryRoundVriable<D>> for FriQueryRoundTarget<D> {
    fn from(value: FriQueryRoundVriable<D>) -> Self {
        Self {
            initial_trees_proof: value.initial_trees_proof.into(),
            steps: value
                .steps
                .into_iter()
                .map(FriQueryStepTarget::from)
                .collect(),
        }
    }
}

impl<const D: usize> From<FriProofTarget<D>> for FriProofVariable<D> {
    fn from(value: FriProofTarget<D>) -> Self {
        Self {
            commit_phase_merkle_caps: value
                .commit_phase_merkle_caps
                .into_iter()
                .map(MerkleCapVariable::from)
                .collect(),
            query_round_proofs: value
                .query_round_proofs
                .into_iter()
                .map(FriQueryRoundVriable::from)
                .collect(),
            final_poly: PolynomialCoeffsExtVariable::from(value.final_poly),
            pow_witness: value.pow_witness.into(),
        }
    }
}

impl<const D: usize> From<FriProofVariable<D>> for FriProofTarget<D> {
    fn from(value: FriProofVariable<D>) -> Self {
        Self {
            commit_phase_merkle_caps: value
                .commit_phase_merkle_caps
                .into_iter()
                .map(MerkleCapTarget::from)
                .collect(),
            query_round_proofs: value
                .query_round_proofs
                .into_iter()
                .map(FriQueryRoundTarget::from)
                .collect(),
            final_poly: PolynomialCoeffsExtTarget::from(value.final_poly),
            pow_witness: value.pow_witness.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use plonky2::fri::proof::FriProofTarget;

    use super::*;
    use crate::prelude::*;

    #[test]
    fn test_conversion() {
        let mut inner_builder = DefaultBuilder::new();
        let a = inner_builder.read::<Variable>();
        let b = inner_builder.read::<Variable>();
        let _ = inner_builder.add(a, b);
        let circuit = inner_builder.build();

        let mut builder = DefaultBuilder::new();

        let proof = builder.api.add_virtual_proof_with_pis(&circuit.data.common);
        let fri_proof = proof.proof.opening_proof;

        let fri_proof_variable = FriProofVariable::from(fri_proof.clone());
        let fri_proof_back = FriProofTarget::from(fri_proof_variable.clone());

        assert_eq!(fri_proof, fri_proof_back);
    }
}
