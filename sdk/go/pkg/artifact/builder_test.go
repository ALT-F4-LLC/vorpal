package artifact

import (
	"bytes"
	"testing"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
)

func TestSecretBuildersCarryTextAsItsBytesAndBytesAsGiven(t *testing.T) {
	binary := []byte{0xff, 0x00, 0xfe, 0x3d, 0x0a}
	want := []*api.ArtifactStepSecret{
		{Name: "BINARY", Value: binary},
		{Name: "TOKEN", Value: []byte("hunter2")},
	}
	systems := []api.ArtifactSystem{api.ArtifactSystem_AARCH64_DARWIN}

	job := NewJob("example", "", systems).
		WithSecrets(map[string]string{"TOKEN": "hunter2"}).
		WithSecretBytes(map[string][]byte{"BINARY": binary, "TOKEN": []byte("ignored")})
	process := NewProcess("example", "bash", systems).
		WithSecrets(map[string]string{"TOKEN": "hunter2"}).
		WithSecretBytes(map[string][]byte{"BINARY": binary, "TOKEN": []byte("ignored")})

	for name, secrets := range map[string]map[string]string{
		"job":     job.Secrets,
		"process": process.Secrets,
	} {
		step := NewArtifactStep("bash").WithSecrets(SecretsToProto(secrets)).Build()

		got := step.GetSecrets()
		if len(got) != len(want) {
			t.Fatalf("%s: step has %d secrets, want %d", name, len(got), len(want))
		}
		for i := range want {
			if got[i].GetName() != want[i].GetName() || !bytes.Equal(got[i].GetValue(), want[i].GetValue()) {
				t.Errorf("%s: secret[%d] = %q %x, want %q %x",
					name, i, got[i].GetName(), got[i].GetValue(), want[i].GetName(), want[i].GetValue())
			}
		}
	}
}
