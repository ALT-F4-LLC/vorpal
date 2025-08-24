package config

import (
	"context"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"os"
	"strings"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/agent"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	apiContext "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/context"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
)

type ConfigContextStore struct {
	artifact map[string]*artifact.Artifact
	variable map[string]string
}

type ConfigContext struct {
	artifact        string
	artifactContext string
	clientAgent     agent.AgentServiceClient
	clientArtifact  artifact.ArtifactServiceClient
	port            int
	store           ConfigContextStore
	system          artifact.ArtifactSystem
	unlock          bool
}

type ConfigServer struct {
	apiContext.UnimplementedContextServiceServer

	store ConfigContextStore
}

func NewConfigServer(store ConfigContextStore) *ConfigServer {
	return &ConfigServer{
		store: store,
	}
}

func (s *ConfigServer) GetArtifact(ctx context.Context, request *artifact.ArtifactRequest) (*artifact.Artifact, error) {
	if request.Digest == "" {
		return nil, fmt.Errorf("'digest' is required")
	}

	response := s.store.artifact[request.Digest]
	if response == nil {
		return nil, fmt.Errorf("artifact not found")
	}

	return response, nil
}

func (s *ConfigServer) GetArtifacts(ctx context.Context, request *artifact.ArtifactsRequest) (*artifact.ArtifactsResponse, error) {
	digests := make([]string, 0)

	for digest := range s.store.artifact {
		digests = append(digests, digest)
	}

	response := &artifact.ArtifactsResponse{
		Digests: digests,
	}

	return response, nil
}

func GetContext() *ConfigContext {
	cmd, err := NewCommand()
	if err != nil {
		log.Fatal(err)
	}

	store := ConfigContextStore{
		artifact: make(map[string]*artifact.Artifact),
		variable: cmd.Variable,
	}

	system, err := GetSystem(cmd.System)
	if err != nil {
		log.Fatalf("failed to get system: %v", err)
	}

	// Load user API token from environment variable
	clientAPIToken, err := LoadClientAPIToken()
	if err != nil {
		log.Fatalf("Failed to load user API token: %v", err)
	}

	caCert, err := os.ReadFile("/var/lib/vorpal/key/ca.pem")
	if err != nil {
		log.Fatalf("Failed to read CA certificate: %v", err)
	}

	caCertPool := x509.NewCertPool()
	if !caCertPool.AppendCertsFromPEM(caCert) {
		log.Fatal("Failed to append CA certificate")
	}

	credentials := credentials.NewTLS(&tls.Config{
		RootCAs:    caCertPool,
		ServerName: "localhost",
	})

	agentHost := strings.ReplaceAll(cmd.Agent, "https://", "")

	clientConnAgent, err := grpc.NewClient(
		agentHost,
		grpc.WithTransportCredentials(credentials),
		grpc.WithUnaryInterceptor(AuthInterceptor(clientAPIToken)),
		grpc.WithStreamInterceptor(StreamAuthInterceptor(clientAPIToken)),
	)
	if err != nil {
		log.Fatalf("failed to connect to agent: %v", err)
	}

	registryHost := strings.ReplaceAll(cmd.Registry, "https://", "")

	clientConnArtifact, err := grpc.NewClient(
		registryHost,
		grpc.WithTransportCredentials(credentials),
		grpc.WithUnaryInterceptor(AuthInterceptor(clientAPIToken)),
		grpc.WithStreamInterceptor(StreamAuthInterceptor(clientAPIToken)),
	)
	if err != nil {
		log.Fatalf("failed to connect to agent: %v", err)
	}

	return &ConfigContext{
		artifact:        cmd.Artifact,
		artifactContext: cmd.ArtifactContext,
		clientAgent:     agent.NewAgentServiceClient(clientConnAgent),
		clientArtifact:  artifact.NewArtifactServiceClient(clientConnArtifact),
		port:            cmd.Port,
		store:           store,
		system:          *system,
		unlock:          cmd.Unlock,
	}
}

func (c *ConfigContext) AddArtifact(artifact *artifact.Artifact) (*string, error) {
	if artifact.Name == "" {
		return nil, fmt.Errorf("'name' is required")
	}

	if len(artifact.Steps) == 0 {
		return nil, fmt.Errorf("'steps' is required")
	}

	if len(artifact.Systems) == 0 {
		return nil, fmt.Errorf("'systems' is required")
	}

	// 1. Setup systems

	artifactJson, err := json.Marshal(artifact)
	if err != nil {
		return nil, err
	}

	artifactDigest := fmt.Sprintf("%x", sha256.Sum256(artifactJson))

	if _, ok := c.store.artifact[artifactDigest]; ok {
		return &artifactDigest, nil
	}

	// TODO: make this run in parallel

	clientReqest := &agent.PrepareArtifactRequest{
		Artifact:        artifact,
		ArtifactContext: c.artifactContext,
	}

	clientResponse, err := c.clientAgent.PrepareArtifact(context.Background(), clientReqest)
	if err != nil {
		return nil, fmt.Errorf("error preparing artifact: %v", err)
	}

	for {
		response, err := clientResponse.Recv()
		if err == io.EOF {
			break
		}

		if err != nil {
			return nil, fmt.Errorf("error receiving response: %v", err)
		}

		if response.ArtifactOutput != nil {
			output := fmt.Sprintf("%s |> %s", artifact.Name, *response.ArtifactOutput)
			println(output)
		}

		if response.Artifact != nil {
			artifact = response.Artifact
		}

		if response.ArtifactDigest != nil {
			artifactDigest = *response.ArtifactDigest
		}
	}

	if _, ok := c.store.artifact[artifactDigest]; !ok {
		c.store.artifact[artifactDigest] = artifact
	}

	return &artifactDigest, nil
}

func fetchArtifacts(client artifact.ArtifactServiceClient, digest string, store map[string]*artifact.Artifact) error {
	if _, ok := store[digest]; ok {
		return nil
	}

	clientResponse, err := client.GetArtifact(context.Background(), &artifact.ArtifactRequest{Digest: digest})
	if err != nil {
		return fmt.Errorf("error fetching artifact: %v", err)
	}

	if _, ok := store[digest]; !ok {
		store[digest] = clientResponse
	}

	for _, step := range clientResponse.Steps {
		if step != nil {
			for _, digest := range step.Artifacts {
				fetchArtifacts(client, digest, store)
			}
		}
	}

	return nil
}

func (c *ConfigContext) FetchArtifact(alias string) (*string, error) {
	request := &artifact.GetArtifactAliasRequest{
		Alias:       alias,
		AliasSystem: c.system,
	}

	response, err := c.clientArtifact.GetArtifactAlias(context.Background(), request)
	if err != nil {
		return nil, fmt.Errorf("error fetching artifact alias: %v", err)
	}

	digest := response.Digest

	if _, ok := c.store.artifact[digest]; ok {
		return &digest, nil
	}

	err = fetchArtifacts(c.clientArtifact, digest, c.store.artifact)
	if err != nil {
		return nil, fmt.Errorf("error fetching '%s': %v", digest, err)
	}

	return &digest, nil
}

func (c *ConfigContext) GetArtifact(digest string) *artifact.Artifact {
	return c.store.artifact[digest]
}

func (c *ConfigContext) GetArtifactContextPath() string {
	return c.artifactContext
}

func (c *ConfigContext) GetArtifactName() string {
	return c.artifact
}

func (c *ConfigContext) GetTarget() artifact.ArtifactSystem {
	return c.system
}

func (c *ConfigContext) GetVariable(name string) *string {
	if _, ok := c.store.variable[name]; !ok {
		return nil
	}

	value := c.store.variable[name]

	return &value
}

func (c *ConfigContext) Run() error {
	var grpcServerOpts []grpc.ServerOption

	grpcServer := grpc.NewServer(grpcServerOpts...)

	apiContext.RegisterContextServiceServer(grpcServer, NewConfigServer(c.store))

	listenerAddr := fmt.Sprintf("[::]:%d", c.port)

	listener, err := net.Listen("tcp", listenerAddr)
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}

	log.Printf("context service: %s", listenerAddr)

	err = grpcServer.Serve(listener)
	if err != nil {
		return err
	}

	return nil
}
