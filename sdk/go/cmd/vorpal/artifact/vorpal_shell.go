package artifact

import (
	"fmt"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func BuildVorpalShell(context *config.ConfigContext) (*string, error) {
	contextTarget := context.GetTarget()

	bun, err := artifact.Bun(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get bun: %w", err)
	}

	crane, err := artifact.Crane(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get crane: %w", err)
	}

	gobin, err := artifact.GoBin(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get go: %w", err)
	}

	goimports, err := artifact.Goimports(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get goimports: %w", err)
	}

	gopls, err := artifact.Gopls(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get gopls: %w", err)
	}

	grpcurl, err := artifact.Grpcurl(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get grpcurl: %w", err)
	}

	nodejs, err := artifact.NodeJS(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get nodejs: %w", err)
	}

	pnpm, err := artifact.Pnpm(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get pnpm: %w", err)
	}

	protoc, err := artifact.Protoc(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get protoc: %w", err)
	}

	protocGenGo, err := artifact.ProtocGenGo(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get protoc-gen-go: %w", err)
	}

	protocGenGoGRPC, err := artifact.ProtocGenGoGRPC(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get protoc-gen-go-grpc: %w", err)
	}

	staticcheck, err := artifact.Staticcheck(context)
	if err != nil {
		return nil, fmt.Errorf("failed to get staticcheck: %w", err)
	}

	goarch, err := language.GetGOARCH(contextTarget)
	if err != nil {
		return nil, fmt.Errorf("failed to get GOARCH for target %s: %w", contextTarget, err)
	}

	goos, err := language.GetGOOS(contextTarget)
	if err != nil {
		return nil, fmt.Errorf("failed to get GOOS for target %s: %w", contextTarget, err)
	}

	return artifact.
		NewProjectEnvironment("vorpal-shell", SYSTEMS).
		WithArtifacts([]*string{
			bun,
			crane,
			gobin,
			goimports,
			gopls,
			grpcurl,
			nodejs,
			pnpm,
			protoc,
			protocGenGo,
			protocGenGoGRPC,
			staticcheck,
		}).
		WithEnvironments([]string{
			"CGO_ENABLED=0",
			fmt.Sprintf("GOARCH=%s", *goarch),
			fmt.Sprintf("GOOS=%s", *goos),
		}).
		Build(context)
}
