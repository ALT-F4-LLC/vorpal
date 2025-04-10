package main

import (
	"log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func main() {
	context := config.GetContext()

	log.Printf("Context target: %v", context.GetTarget())

	gobin, err := context.FetchArtifact("6f024d78f0957297229cb00b74b9544fb2c4708a465a584b1e02dfbe5f71922b")
	if err != nil {
		log.Fatalf("failed to fetch go artifact: %v", err)
	}

	goimports, err := context.FetchArtifact("112f8c42be33bfa5274fcdff2748cd68eae755adbae1cbd70cc012531375d7c1")
	if err != nil {
		log.Fatalf("failed to fetch goimports artifact: %v", err)
	}

	gopls, err := context.FetchArtifact("91cc5dc188bc8bc5e6ff63aede51540ae15c7980ab898e1b0afb6db79657fa43")
	if err != nil {
		log.Fatalf("failed to fetch gopls artifact: %v", err)
	}

	protoc, err := context.FetchArtifact("8ad451bdcda8f24f4af59ccca23fd71a06975a9d069571f19b9a0d503f8a65c8")
	if err != nil {
		log.Fatalf("failed to fetch protoc artifact: %v", err)
	}

	protocGenGo, err := context.FetchArtifact("47a94d59d206be31eef2214418fce60570e7a9a175f96eeab02d1c9c3c7d0ed9")
	if err != nil {
		log.Fatalf("failed to fetch protoc-gen-go artifact: %v", err)
	}

	protocGenGoGRPC, err := context.FetchArtifact("d4165e4b2ca1b82ddaed218d948ce10eca9a714405575ffccb0674eb069e3361")
	if err != nil {
		log.Fatalf("failed to fetch protoc-gen-go-grpc artifact: %v", err)
	}

	vorpalShellArtifacts := []*string{
		gobin,
		goimports,
		gopls,
		protoc,
		protocGenGo,
		protocGenGoGRPC,
	}

	vorpalShellBuilder := language.NewRustShellBuilder("vorpal-shell")
	vorpalShellBuilder = vorpalShellBuilder.WithArtifacts(vorpalShellArtifacts)

	vorpalShell, err := vorpalShellBuilder.Build(context)
	if err != nil {
		log.Fatalf("Failed to build vorpal shell: %v", err)
	}

	err = context.Run([]*string{vorpalShell})
	if err != nil {
		log.Fatalf("failed to run vorpal configuration: %v", err)
	}
}
