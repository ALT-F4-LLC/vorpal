package language

import (
	"fmt"
	"slices"
	"strings"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type TypeScript struct {
	aliases       []string
	artifacts     []*string
	entrypoint    *string
	environments  []string
	includes      []string
	name          string
	nodeModules   map[string]*string
	secrets       []*api.ArtifactStepSecret
	sourceScripts []string
	systems       []api.ArtifactSystem
	vorpalSdk     bool
	workingDir    *string
}

func NewTypeScript(name string, systems []api.ArtifactSystem) *TypeScript {
	return &TypeScript{
		aliases:       []string{},
		artifacts:     []*string{},
		entrypoint:    nil,
		environments:  []string{},
		includes:      []string{},
		name:          name,
		nodeModules:   map[string]*string{},
		secrets:       []*api.ArtifactStepSecret{},
		sourceScripts: []string{},
		systems:       systems,
		vorpalSdk:     true,
		workingDir:    nil,
	}
}

func (b *TypeScript) WithAliases(aliases []string) *TypeScript {
	for _, alias := range aliases {
		if !slices.Contains(b.aliases, alias) {
			b.aliases = append(b.aliases, alias)
		}
	}
	return b
}

func (b *TypeScript) WithArtifacts(artifacts []*string) *TypeScript {
	b.artifacts = artifacts
	return b
}

func (b *TypeScript) WithEntrypoint(entrypoint string) *TypeScript {
	b.entrypoint = &entrypoint
	return b
}

func (b *TypeScript) WithEnvironments(environments []string) *TypeScript {
	b.environments = environments
	return b
}

func (b *TypeScript) WithIncludes(includes []string) *TypeScript {
	b.includes = includes
	return b
}

func (b *TypeScript) WithNodeModules(modules map[string]*string) *TypeScript {
	for name, digest := range modules {
		b.nodeModules[name] = digest
	}
	return b
}

func (b *TypeScript) WithSecrets(secrets map[string]string) *TypeScript {
	for _, name := range artifact.SortedKeys(secrets) {
		value := secrets[name]
		secret := &api.ArtifactStepSecret{
			Name:  name,
			Value: value,
		}

		if slices.ContainsFunc(b.secrets, func(s *api.ArtifactStepSecret) bool { return s.Name == name }) {
			continue
		}

		b.secrets = append(b.secrets, secret)
	}

	return b
}

func (b *TypeScript) WithSourceScripts(scripts []string) *TypeScript {
	for _, script := range scripts {
		if !slices.Contains(b.sourceScripts, script) {
			b.sourceScripts = append(b.sourceScripts, script)
		}
	}
	return b
}

func (b *TypeScript) WithVorpalSdk(include bool) *TypeScript {
	b.vorpalSdk = include
	return b
}

func (b *TypeScript) WithWorkingDir(dir string) *TypeScript {
	b.workingDir = &dir
	return b
}

func (builder *TypeScript) Build(context *config.ConfigContext) (*string, error) {
	// Sort secrets for deterministic output
	slices.SortFunc(builder.secrets, func(a, b *api.ArtifactStepSecret) int {
		return strings.Compare(a.Name, b.Name)
	})

	// Resolve Bun artifact
	bunDigest, err := artifact.Bun(context)
	if err != nil {
		return nil, err
	}

	bunBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*bunDigest))

	// Build source
	sourcePath := "."

	sourceBuilder := artifact.NewArtifactSource(builder.name, sourcePath)

	if len(builder.includes) > 0 {
		sourceBuilder = sourceBuilder.WithIncludes(builder.includes)
	}

	src := sourceBuilder.Build()
	source := &src

	// Setup step source directory
	stepSourceDir := fmt.Sprintf("%s/source/%s", sourcePath, source.Name)

	if builder.workingDir != nil {
		stepSourceDir = fmt.Sprintf("%s/%s", stepSourceDir, *builder.workingDir)
	}

	// Setup Vorpal SDK
	if builder.vorpalSdk {
		vorpalSdk, err := context.FetchArtifactAlias("library/vorpal-sdk-typescript:latest")
		if err != nil {
			return nil, err
		}

		builder.artifacts = append(builder.artifacts, vorpalSdk)
		builder.nodeModules["@vorpal/sdk"] = vorpalSdk
	}

	// Setup node modules - package.json rewriting script
	var stepPackageJsonJsParts []string

	stepPackageJsonJsParts = append(stepPackageJsonJsParts, "const fs=require('fs')")
	stepPackageJsonJsParts = append(stepPackageJsonJsParts, "const p=JSON.parse(fs.readFileSync('package.json','utf8'))")

	for _, packageName := range artifact.SortedKeys(builder.nodeModules) {
		digest := builder.nodeModules[packageName]
		if digest == nil {
			return nil, fmt.Errorf("node module %q has nil digest", packageName)
		}
		envKey := artifact.GetEnvKey(*digest)

		stepPackageJsonJsParts = append(stepPackageJsonJsParts,
			fmt.Sprintf("if(p.dependencies?.['%s'])p.dependencies['%s']='file:%s'", packageName, packageName, envKey))

		stepPackageJsonJsParts = append(stepPackageJsonJsParts,
			fmt.Sprintf("if(p.devDependencies?.['%s'])p.devDependencies['%s']='file:%s'", packageName, packageName, envKey))
	}

	stepPackageJsonJsParts = append(stepPackageJsonJsParts, "fs.writeFileSync('package.json',JSON.stringify(p,null,2))")

	stepPackageJsonJs := strings.Join(stepPackageJsonJsParts, ";") + ";"
	stepPackageJsonScript := fmt.Sprintf("%s/bun -e \"%s\"\n", bunBin, stepPackageJsonJs)

	// Setup node modules - bun.lock rewriting script
	var stepBunLockJsParts []string

	stepBunLockJsParts = append(stepBunLockJsParts, "const fs=require('fs')")
	stepBunLockJsParts = append(stepBunLockJsParts, "if(fs.existsSync('bun.lock')){var t=fs.readFileSync('bun.lock','utf8');var q=String.fromCharCode(34)")

	for _, packageName := range artifact.SortedKeys(builder.nodeModules) {
		digest := builder.nodeModules[packageName]
		if digest == nil {
			return nil, fmt.Errorf("node module %q has nil digest", packageName)
		}
		envKey := artifact.GetEnvKey(*digest)

		// Replace workspace dependency value: "package": "file:/old" -> "package": "file:<env_key>"
		stepBunLockJsParts = append(stepBunLockJsParts,
			fmt.Sprintf("var p1=q+'%s'+q+': '+q+'file:';var i=t.indexOf(p1);while(i>=0){var s=i+p1.length;var e=t.indexOf(q,s);t=t.substring(0,s)+'%s'+t.substring(e);i=t.indexOf(p1,s)}", packageName, envKey))

		// Replace packages resolved specifier: "package@file:/old" -> "package@file:<env_key>"
		stepBunLockJsParts = append(stepBunLockJsParts,
			fmt.Sprintf("var p2=q+'%s@file:';var i=t.indexOf(p2);while(i>=0){var s=i+p2.length;var e=t.indexOf(q,s);t=t.substring(0,s)+'%s'+t.substring(e);i=t.indexOf(p2,s)}", packageName, envKey))
	}

	stepBunLockJsParts = append(stepBunLockJsParts, "fs.writeFileSync('bun.lock',t)}")

	stepBunLockJs := strings.Join(stepBunLockJsParts, ";") + ";"
	stepBunLockScript := fmt.Sprintf("%s/bun -e \"%s\"\n", bunBin, stepBunLockJs)

	// Setup build command
	var stepBuildCommand string

	if builder.entrypoint != nil {
		stepBuildCommand = fmt.Sprintf("mkdir -p $VORPAL_OUTPUT/bin\n\n%s/bun build --compile %s --outfile %s\n\ncp %s $VORPAL_OUTPUT/bin/%s",
			bunBin, *builder.entrypoint, builder.name, builder.name, builder.name)
	} else {
		stepBuildCommand = fmt.Sprintf("mkdir -p $VORPAL_OUTPUT\n\n%s/bun x tsc --project tsconfig.json --outDir dist\n\ncp package.json $VORPAL_OUTPUT/\ncp -r dist $VORPAL_OUTPUT/\ncp -r node_modules $VORPAL_OUTPUT/",
			bunBin)
	}

	// Build step script
	stepSourceScripts := strings.Join(builder.sourceScripts, "\n")

	stepScript := fmt.Sprintf("pushd %s\n\n%s\n%s\n%s\n\n%s/bun install --frozen-lockfile\n\n%s",
		stepSourceDir, stepSourceScripts, stepPackageJsonScript, stepBunLockScript, bunBin, stepBuildCommand)

	// Build artifacts list
	var artifacts []*string
	artifacts = append(artifacts, bunDigest)
	artifacts = append(artifacts, builder.artifacts...)

	// Add node module artifact digests
	for _, packageName := range artifact.SortedKeys(builder.nodeModules) {
		digest := builder.nodeModules[packageName]
		if digest == nil {
			return nil, fmt.Errorf("node module %q has nil digest", packageName)
		}
		artifacts = append(artifacts, digest)
	}

	// Build environments
	environments := []string{
		fmt.Sprintf("PATH=%s", bunBin),
	}

	for _, env := range builder.environments {
		environments = append(environments, env)
	}

	// Create step and artifact
	sources := []*api.ArtifactSource{source}

	step, err := artifact.Shell(context, artifacts, environments, stepScript, builder.secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return artifact.NewArtifact(builder.name, steps, builder.systems).
		WithAliases(builder.aliases).
		WithSources(sources).
		Build(context)
}

// ---------------------------------------------------------------------------
// TypeScript Development Environment
// ---------------------------------------------------------------------------

type TypeScriptDevelopmentEnvironment struct {
	artifacts    []*string
	environments []string
	name         string
	nodeModules  map[string]*string
	secrets      map[string]string
	systems      []api.ArtifactSystem
}

func NewTypeScriptDevelopmentEnvironment(name string, systems []api.ArtifactSystem) *TypeScriptDevelopmentEnvironment {
	return &TypeScriptDevelopmentEnvironment{
		artifacts:    []*string{},
		environments: []string{},
		name:         name,
		nodeModules:  map[string]*string{},
		secrets:      map[string]string{},
		systems:      systems,
	}
}

func (b *TypeScriptDevelopmentEnvironment) WithArtifacts(artifacts []*string) *TypeScriptDevelopmentEnvironment {
	b.artifacts = append(b.artifacts, artifacts...)
	return b
}

func (b *TypeScriptDevelopmentEnvironment) WithEnvironments(environments []string) *TypeScriptDevelopmentEnvironment {
	b.environments = append(b.environments, environments...)
	return b
}

func (b *TypeScriptDevelopmentEnvironment) WithNodeModule(packageName string, digest *string) *TypeScriptDevelopmentEnvironment {
	b.nodeModules[packageName] = digest
	return b
}

func (b *TypeScriptDevelopmentEnvironment) WithSecrets(secrets map[string]string) *TypeScriptDevelopmentEnvironment {
	for k, v := range secrets {
		if _, exists := b.secrets[k]; !exists {
			b.secrets[k] = v
		}
	}
	return b
}

func (b *TypeScriptDevelopmentEnvironment) Build(context *config.ConfigContext) (*string, error) {
	bun, err := artifact.Bun(context)
	if err != nil {
		return nil, err
	}

	artifacts := []*string{bun}
	artifacts = append(artifacts, b.artifacts...)

	environments := make([]string, len(b.environments))
	copy(environments, b.environments)

	// Add node module artifacts and NODE_PATH entries
	if len(b.nodeModules) > 0 {
		var nodePaths []string

		for _, packageName := range artifact.SortedKeys(b.nodeModules) {
			digest := b.nodeModules[packageName]
			if digest != nil {
				artifacts = append(artifacts, digest)
				nodePaths = append(nodePaths, fmt.Sprintf("%s/..", artifact.GetEnvKey(*digest)))
			}
		}

		if len(nodePaths) > 0 {
			environments = append(environments, fmt.Sprintf("NODE_PATH=%s", strings.Join(nodePaths, ":")))
		}
	}

	devenv := artifact.NewDevelopmentEnvironment(b.name, b.systems).
		WithArtifacts(artifacts).
		WithEnvironments(environments)

	if len(b.secrets) > 0 {
		devenv = devenv.WithSecrets(b.secrets)
	}

	return devenv.Build(context)
}
