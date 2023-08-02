// The API for operations related to SSZ, a serialization method used by the Ethereum consensus
// layer or "Beacon Chain".
package ssz

import (
	"github.com/succinctlabs/gnark-gadgets/hash/sha256"
	"github.com/succinctlabs/gnark-gadgets/succinct"
	"github.com/succinctlabs/gnark-gadgets/vars"
)

// SimpleSerializeAPI is a wrapper around succinct.API that provides methods related to
// SSZ, a serialization method used on the Beacon Chain. For more information and details, see:
// https://ethereum.org/en/developers/docs/data-structures-and-encoding/ssz/
type SimpleSerializeAPI struct {
	api succinct.API
}

// Creates a new SimpleSerializeAPI.
func NewAPI(api *succinct.API) *SimpleSerializeAPI {
	return &SimpleSerializeAPI{api: *api}
}

// Verifies an ssz proof with a gindex that is a compile time constant.
func (a *SimpleSerializeAPI) VerifyProof(
	root [32]vars.Byte,
	leaf [32]vars.Byte,
	proof [][32]vars.Byte,
	gindex int,
) {
	restoredRoot := a.RestoreMerkleRoot(leaf, proof, gindex)
	for i := 0; i < 32; i++ {
		a.api.FrontendAPI().AssertIsEqual(root[i].Value, restoredRoot[i].Value)
	}
}

// Verifies an ssz proof with a gindex that is a circuit variable.
func (a *SimpleSerializeAPI) VerifyProofWithGIndexVariable(
	root [32]vars.Byte,
	leaf [32]vars.Byte,
	proof [][32]vars.Byte,
	gindex vars.U64,
) {
	restoredRoot := a.RestoreMerkleRootWithGIndexVariable(leaf, proof, gindex)
	for i := 0; i < 32; i++ {
		a.api.FrontendAPI().AssertIsEqual(root[i].Value, restoredRoot[i].Value)
	}
}

func (a *SimpleSerializeAPI) RestoreMerkleRootWithGIndexVariable(
	leaf [32]vars.Byte,
	proof [][32]vars.Byte,
	gindex vars.U64,
) [32]vars.Byte {
	gindexBits := a.api.ToBinaryLE(gindex.Value, len(proof)+1)
	hash := leaf
	for i := 0; i < len(proof); i++ {
		hash1 := sha256.Hash(a.api, append(proof[i][:], hash[:]...))
		hash2 := sha256.Hash(a.api, append(hash[:], proof[i][:]...))
		hash = a.api.SelectBytes32(gindexBits[i], hash1, hash2)
	}
	return hash
}

func (a *SimpleSerializeAPI) RestoreMerkleRoot(
	leaf [32]vars.Byte,
	proof [][32]vars.Byte,
	gindex int,
) [32]vars.Byte {
	hash := leaf
	for i := 0; i < len(proof); i++ {
		if gindex%2 == 1 {
			hash = sha256.Hash(a.api, append(proof[i][:], hash[:]...))
		} else {
			hash = sha256.Hash(a.api, append(hash[:], proof[i][:]...))
		}
		gindex = gindex / 2
	}
	return hash
}

func (a *SimpleSerializeAPI) HashTreeRoot(
	leaves [][32]vars.Byte,
	nbLeaves int,
) [32]vars.Byte {
	if nbLeaves&(nbLeaves-1) != 0 {
		panic("nbLeaves must be a power of 2")
	}
	for nbLeaves > 1 {
		for i := 0; i < nbLeaves/2; i++ {
			leaves[i] = sha256.Hash(a.api, append(leaves[i*2][:], leaves[i*2+1][:]...))
		}
		nbLeaves = nbLeaves / 2
	}
	return leaves[0]
}
