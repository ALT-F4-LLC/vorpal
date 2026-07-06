//go:build ignore

// Builder-output parity golden generator (reference / reproducibility copy).
//
// digests.json is produced by running the Go SDK builders' PURE pipeline
// (Shell->Bash on a Darwin target, then serialize+sha256 the input Artifact
// exactly as ConfigContext.AddArtifact does before the gRPC round-trip). The
// Python builder-output parity test (tests/test_builder_parity.py) reproduces
// the same inputs and asserts its builders yield these Go-produced digests, so
// a Python builder that diverges on env ordering, default step-script text, or
// secret sort fails BEFORE the CLI-gated cross-language e2e.
//
// REGENERATE (after a deliberate, cross-SDK-coordinated builder change):
//   cp gen_goldens.go <repo>/sdk/go/cmd/_genparity/main.go   # drop //go:build ignore
//   (cd <repo>/sdk/go && go run ./cmd/_genparity) > new.json
//   # copy the {"digests": {...}} object into digests.json, then rm the cmd dir.
// Never edit digests.json by hand to match a Python change in isolation — that
// silently breaks parity with the Go/TS/Rust SDKs.

package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	art "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

const darwin = api.ArtifactSystem_AARCH64_DARWIN

func ptr(s string) *string { return &s }

func digest(a *api.Artifact) string {
	j, err := config.SerializeArtifactJSON(a)
	if err != nil {
		panic(err)
	}
	return fmt.Sprintf("%x", sha256.Sum256(j))
}

func render(tmpl string, vars any) string {
	t := template.Must(template.New("s").Parse(tmpl))
	var buf bytes.Buffer
	if err := t.Execute(&buf, vars); err != nil {
		panic(err)
	}
	return buf.String()
}

func mkArtifact(name string, step *api.ArtifactStep, systems []api.ArtifactSystem, sources []*api.ArtifactSource, aliases []string) *api.Artifact {
	return &api.Artifact{
		Aliases: aliases,
		Name:    name,
		Sources: sources,
		Steps:   []*api.ArtifactStep{step},
		Systems: systems,
		Target:  darwin,
	}
}

func main() {
	out := map[string]string{}

	// 1. job: secret sort (A before B) + bash env/PATH ordering + artifacts.
	{
		secrets := art.SecretsToProto(map[string]string{"B_KEY": "b", "A_KEY": "a"})
		step, err := art.Bash([]*string{ptr("digabc")}, []string{}, "echo hello", secrets)
		if err != nil {
			panic(err)
		}
		a := mkArtifact("vorpal-job-test", step, []api.ArtifactSystem{darwin}, []*api.ArtifactSource{}, []string{})
		out["job"] = digest(a)
	}

	// 2. process: process script template + args + artifacts PATH.
	{
		arguments := []string{"--port", "8080"}
		artifacts := []*string{ptr("dig1"), ptr("dig2")}
		artifactBins := []string{}
		for _, d := range artifacts {
			artifactBins = append(artifactBins, fmt.Sprintf("$VORPAL_ARTIFACT_%s/bin", *d))
		}
		script := render(art.ProcessScriptTemplate, art.ProcessScriptTemplateVars{
			Arguments:  strings.Join(arguments, " "),
			Artifacts:  strings.Join(artifactBins, ":"),
			Entrypoint: "/bin/server",
			Name:       "proc",
		})
		secrets := art.SecretsToProto(map[string]string{"TOKEN": "x"})
		step, err := art.Bash(artifacts, []string{}, script, secrets)
		if err != nil {
			panic(err)
		}
		a := mkArtifact("proc", step, []api.ArtifactSystem{darwin}, []*api.ArtifactSource{}, []string{})
		out["process"] = digest(a)
	}

	// 3. devenv: template + env backup/export/restore/unset + PATH merge.
	{
		environments := []string{"FOO=bar", "PATH=/custom/bin"}
		artifacts := []*string{ptr("digtool")}
		backups := []string{
			"export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"",
			"export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"",
			"export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"",
		}
		exports := []string{
			"export PS1=\"(dev) $PS1\"",
			"export VORPAL_SHELL=\"1\"",
		}
		restores := []string{
			"export PATH=\"$VORPAL_SHELL_BACKUP_PATH\"",
			"export PS1=\"$VORPAL_SHELL_BACKUP_PS1\"",
			"export VORPAL_SHELL=\"$VORPAL_SHELL_BACKUP_VORPAL_SHELL\"",
		}
		unsets := []string{
			"unset VORPAL_SHELL_BACKUP_PATH",
			"unset VORPAL_SHELL_BACKUP_PS1",
			"unset VORPAL_SHELL_BACKUP_VORPAL_SHELL",
		}
		for _, envvar := range environments {
			parts := strings.SplitN(envvar, "=", 2)
			key := parts[0]
			if key == "PATH" {
				continue
			}
			backups = append(backups, fmt.Sprintf("export VORPAL_SHELL_BACKUP_%s=\"$%s\"", key, key))
			exports = append(exports, fmt.Sprintf("export %s", envvar))
			restores = append(restores, fmt.Sprintf("export %s=\"$VORPAL_SHELL_BACKUP_%s\"", key, key))
			unsets = append(unsets, fmt.Sprintf("unset VORPAL_SHELL_BACKUP_%s", key))
		}
		stepPathArtifacts := []string{}
		for _, d := range artifacts {
			stepPathArtifacts = append(stepPathArtifacts, fmt.Sprintf("%s/bin", art.GetEnvKey(*d)))
		}
		stepPath := strings.Join(stepPathArtifacts, ":")
		for _, envvar := range environments {
			if pathValue, ok := strings.CutPrefix(envvar, "PATH="); ok {
				stepPath = fmt.Sprintf("%s:%s", pathValue, stepPath)
			}
		}
		exports = append(exports, fmt.Sprintf("export PATH=%s:$PATH", stepPath))
		script := render(art.ScriptDevelopmentEnvironmentTemplate, art.DevelopmentEnvironmentTemplateArgs{
			Backups:  strings.Join(backups, "\n"),
			Exports:  strings.Join(exports, "\n"),
			Restores: strings.Join(restores, "\n"),
			Unsets:   strings.Join(unsets, "\n"),
		})
		secrets := art.SecretsToProto(map[string]string{"S": "v"})
		step, err := art.Bash(artifacts, []string{}, script, secrets)
		if err != nil {
			panic(err)
		}
		a := mkArtifact("dev", step, []api.ArtifactSystem{darwin}, []*api.ArtifactSource{}, []string{})
		out["devenv"] = digest(a)
	}

	// 4. userenv: symlink sort by source + template (empty environments).
	{
		artifacts := []*string{ptr("diguser")}
		symlinks := map[string]string{"/src/b": "/dst/b", "/src/a": "/dst/a"}
		stepPathArtifacts := []string{}
		for _, d := range artifacts {
			stepPathArtifacts = append(stepPathArtifacts, fmt.Sprintf("%s/bin", art.GetEnvKey(*d)))
		}
		stepPath := strings.Join(stepPathArtifacts, ":")
		symlinksActivate := []string{}
		symlinksCheck := []string{}
		symlinksDeactivate := []string{}
		for _, source := range art.SortedKeys(symlinks) {
			target := symlinks[source]
			symlinksActivate = append(symlinksActivate, fmt.Sprintf("ln -s %s %s", source, target))
			symlinksCheck = append(symlinksCheck, fmt.Sprintf("if [ -f %s ]; then echo \"ERROR: Symlink target exists -> %s\" && exit 1; fi", target, target))
			symlinksDeactivate = append(symlinksDeactivate, fmt.Sprintf("rm -f %s", target))
		}
		script := render(art.ScriptUserEnvironmentTemplate, art.UserEnvironmentTemplateArgs{
			Environments:       "",
			Path:               stepPath,
			SymlinksActivate:   strings.Join(symlinksActivate, "\n"),
			SymlinksCheck:      strings.Join(symlinksCheck, "\n"),
			SymlinksDeactivate: strings.Join(symlinksDeactivate, "\n"),
		})
		step, err := art.Bash(artifacts, []string{}, script, nil)
		if err != nil {
			panic(err)
		}
		a := mkArtifact("user", step, []api.ArtifactSystem{darwin}, []*api.ArtifactSource{}, []string{})
		out["userenv"] = digest(a)
	}

	// 5. artifact-sources: source dedup by name + alias dedup + bash env/secret.
	{
		secrets := art.SecretsToProto(map[string]string{"KEY": "val"})
		step, err := art.Bash([]*string{ptr("digart")}, []string{"ENVKEY=enval"}, "echo s", secrets)
		if err != nil {
			panic(err)
		}
		src1 := &api.ArtifactSource{Digest: nil, Excludes: []string{}, Includes: []string{}, Name: "s1", Path: "/p1"}
		src2 := &api.ArtifactSource{Digest: ptr("srcdig"), Excludes: []string{"*.log"}, Includes: []string{"src/**"}, Name: "s2", Path: "/p2"}
		sources := []*api.ArtifactSource{src1, src2}
		a := mkArtifact("multi", step, []api.ArtifactSystem{darwin}, sources, []string{"x", "y"})
		out["artifact-sources"] = digest(a)
	}

	enc, err := json.MarshalIndent(map[string]any{"digests": out}, "", "  ")
	if err != nil {
		panic(err)
	}
	fmt.Println(string(enc))
}
