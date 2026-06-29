from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Optional as _Optional

DESCRIPTOR: _descriptor.FileDescriptor

class ArchivePullRequest(_message.Message):
    __slots__ = ("digest", "namespace")
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    digest: str
    namespace: str
    def __init__(self, digest: _Optional[str] = ..., namespace: _Optional[str] = ...) -> None: ...

class ArchivePushRequest(_message.Message):
    __slots__ = ("data", "digest", "namespace")
    DATA_FIELD_NUMBER: _ClassVar[int]
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    NAMESPACE_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    digest: str
    namespace: str
    def __init__(self, data: _Optional[bytes] = ..., digest: _Optional[str] = ..., namespace: _Optional[str] = ...) -> None: ...

class ArchiveResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ArchivePullResponse(_message.Message):
    __slots__ = ("data",)
    DATA_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    def __init__(self, data: _Optional[bytes] = ...) -> None: ...
