package main

import (
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func newShellArtifacts(context *config.ConfigContext) ([]*string, error) {
	gobin, err := artifact.GoBin(context)
	if err != nil {
		return nil, err
	}

	goimports, err := artifact.Goimports(context)
	if err != nil {
		return nil, err
	}

	gopls, err := artifact.Gopls(context)
	if err != nil {
		return nil, err
	}

	grpcurl, err := artifact.Grpcurl(context)
	if err != nil {
		return nil, err
	}

	protoc, err := artifact.Protoc(context)
	if err != nil {
		return nil, err
	}

	protocGenGo, err := artifact.ProtocGenGo(context)
	if err != nil {
		return nil, err
	}

	protocGenGoGRPC, err := artifact.ProtocGenGoGRPC(context)
	if err != nil {
		return nil, err
	}

	return []*string{
		gobin,
		goimports,
		gopls,
		grpcurl,
		protoc,
		protocGenGo,
		protocGenGoGRPC,
	}, nil
}

func newArtifacts(context *config.ConfigContext) ([]*string, error) {
	protoc, err := artifact.Protoc(context)
	if err != nil {
		return nil, err
	}

	return []*string{protoc}, nil
}
