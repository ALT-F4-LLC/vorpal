let { Lock, .. } = import "@vorpal/schema.ncl" in

{
  sandbox = {
    aarch64-linux = "",
    aarch64-macos = "819232062aecd85c775f498b2cdb7f4bf0b8347b0a1144658a0d30c2cfebb744",
    x86_64-linux = "",
    x86_64-macos = "",
  }
} | Lock
