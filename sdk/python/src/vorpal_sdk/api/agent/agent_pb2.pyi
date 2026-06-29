from vorpal_sdk.api.artifact import artifact_pb2 as _artifact_pb2
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class PrepareArtifactRequest(_message.Message):
    __slots__ = ("artifact_unlock", "artifact_context", "artifact_namespace", "registry", "artifact")
    ARTIFACT_UNLOCK_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_CONTEXT_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    REGISTRY_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    artifact_unlock: bool
    artifact_context: str
    artifact_namespace: str
    registry: str
    artifact: _artifact_pb2.Artifact
    def __init__(self, artifact_unlock: _Optional[bool] = ..., artifact_context: _Optional[str] = ..., artifact_namespace: _Optional[str] = ..., registry: _Optional[str] = ..., artifact: _Optional[_Union[_artifact_pb2.Artifact, _Mapping]] = ...) -> None: ...

class PrepareArtifactResponse(_message.Message):
    __slots__ = ("artifact_digest", "artifact_output", "artifact")
    ARTIFACT_DIGEST_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_OUTPUT_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    artifact_digest: str
    artifact_output: str
    artifact: _artifact_pb2.Artifact
    def __init__(self, artifact_digest: _Optional[str] = ..., artifact_output: _Optional[str] = ..., artifact: _Optional[_Union[_artifact_pb2.Artifact, _Mapping]] = ...) -> None: ...
