package main

import (
	_ "embed"
	"flag"
	"os"

	"github.com/consensys/gnark/backend/plonk"
	"github.com/consensys/gnark/logger"
)

func main() {
	circuitPath := flag.String("circuit", "", "circuit data directory")
	dataPath := flag.String("data", "", "data directory")
	proofFlag := flag.Bool("prove", false, "create a proof")
	verifyFlag := flag.Bool("verify", false, "verify a proof")
	compileFlag := flag.Bool("compile", false, "Compile and save the universal verifier circuit")
	contractFlag := flag.Bool("contract", true, "Generate solidity contract")
	flag.Parse()

	log := logger.Logger()

	if *circuitPath == "" {
		log.Error().Msg("please specify a path to circuit dir (containing verifier_only_circuit_data and proof_with_public_inputs)")
		os.Exit(1)
	}

	if *dataPath == "" {
		log.Error().Msg("please specify a path to data dir (where the compiled gnark circuit data will be)")
		os.Exit(1)
	}

	log.Debug().Msg("Circuit path: " + *circuitPath)
	log.Debug().Msg("Data path: " + *dataPath)

	if *compileFlag {
		log.Info().Msg("compiling verifier circuit")
		r1cs, pk, vk, err := CompileVerifierCircuit("./data/dummy")
		if err != nil {
			log.Error().Msg("failed to compile verifier circuit:" + err.Error())
			os.Exit(1)
		}
		err = SaveVerifierCircuit(*dataPath, r1cs, pk, vk)
		if err != nil {
			log.Error().Msg("failed to save verifier circuit:" + err.Error())
			os.Exit(1)
		}

		if *contractFlag {
			log.Info().Msg("generating solidity contract")
			err := ExportIFunctionVerifierSolidity(*dataPath, vk)
			if err != nil {
				log.Error().Msg("failed to generate solidity contract:" + err.Error())
				os.Exit(1)
			}
		}
	}

	if *proofFlag {
		log.Info().Msg("loading the plonk proving key and circuit data")
		r1cs, pk, err := LoadProverData(*dataPath)
		if err != nil {
			log.Err(err).Msg("failed to load the verifier circuit")
			os.Exit(1)
		}
		log.Info().Msg("creating the plonk verifier proof")
		proof, publicWitness, err := Prove(*circuitPath, r1cs, pk)
		if err != nil {
			log.Err(err).Msg("failed to create the proof")
			os.Exit(1)
		}

		log.Info().Msg("loading the proof, verifying key and verifying proof")
		vk, err := LoadVerifierKey(*dataPath)
		if err != nil {
			log.Err(err).Msg("failed to load the verifier key")
			os.Exit(1)
		}
		err = plonk.Verify(proof, vk, publicWitness)
		if err != nil {
			log.Err(err).Msg("failed to verify proof")
			os.Exit(1)
		}
		log.Info().Msg("Successfully verified proof")
	}

	if *verifyFlag {
		log.Info().Msg("loading the proof, verifying key and public inputs")
		vk, err := LoadVerifierKey(*dataPath)
		if err != nil {
			log.Err(err).Msg("failed to load the verifier key")
			os.Exit(1)
		}
		publicWitness, err := LoadPublicWitness(*circuitPath)
		if err != nil {
			log.Err(err).Msg("failed to load the public witness")
			os.Exit(1)
		}

		proof, err := LoadProof()
		if err != nil {
			log.Err(err).Msg("failed to load the proof")
			os.Exit(1)
		}
		err = plonk.Verify(proof, vk, publicWitness)
		if err != nil {
			log.Err(err).Msg("failed to verify proof")
			os.Exit(1)
		}
		log.Info().Msg("Successfully verified proof")
	}
}
