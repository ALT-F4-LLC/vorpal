// Prism (docs-redesign stage 3 signature 2, rev 2; ported once, stage 4
// disposition M4): one artifact, `hello`, written with each SDK's builders,
// and the canonical JSON every SDK serialises it to. Pure functions only, so
// Prism.astro renders the end state at build time with the same code the
// browser runs (src/scripts/prism.js).
//
// The JSON shape and field order are the SDKs' shared serializer (target,
// sources, steps, systems, aliases, name; enums as integers, no
// whitespace). The default digest, 43f5c139…7153d, matches the Python SDK's
// compute_artifact_digest for this definition.

export var SYS = ["aarch64-darwin", "aarch64-linux", "x86_64-darwin", "x86_64-linux"];
export var LANGS = [
  { id: "python", name: "Python", pkg: "from vorpal_sdk" },
  { id: "typescript", name: "TypeScript", pkg: "@altf4llc/vorpal-sdk" },
  { id: "go", name: "Go", pkg: "sdk/go/pkg/artifact" },
  { id: "rust", name: "Rust", pkg: "use vorpal_sdk" }
];
var NAMES = {
  python: [["script", "source", "step", "hello"], ["cmd", "src", "build_step", "hello_app"]],
  go: [["script", "source", "step", "hello"], ["cmd", "src", "buildStep", "helloApp"]],
  rust: [["script", "source", "step", "hello"], ["cmd", "src", "build_step", "hello_app"]],
  typescript: [["script", "source", "step", "hello"], ["cmd", "src", "buildStep", "helloApp"]]
};
var KW = { python: "", go: "", rust: "let mut", typescript: "const await new" };
var SCRIPT = "cp -pr source/hello/. $VORPAL_OUTPUT";
/* @S @R @T @A: variable names. [V]: version slot. [I]: include slot. */
var TPL = {
  python: [
    { q: '"', inc: ', "docs"', c: '@S = "' + SCRIPT + '"\n@R = (\n    ArtifactSource("hello", ".")\n    .with_includes(["src"[I]])\n    .build()\n)\n@T = (\n    ArtifactStep("bash")\n    .with_script(@S)\n    .build()\n)\n@A = (\n    Artifact("hello", [@T], SYSTEMS)\n    .with_aliases(["hello:[V]"])\n    .with_sources([@R])\n    .build(ctx)\n)' },
    { q: "'", inc: ", 'docs'", c: "@S = '" + SCRIPT + "'\n@R = ArtifactSource('hello', '.') \\\n    .with_includes(['src'[I]]) \\\n    .build()\n@T = ArtifactStep('bash').with_script(@S) \\\n    .build()\n@A = Artifact('hello', [@T], SYSTEMS) \\\n    .with_sources([@R]) \\\n    .with_aliases(['hello:[V]']) \\\n    .build(ctx)" }
  ],
  go: [
    { q: '"', inc: ', "docs"', c: '@S := "' + SCRIPT + '"\n@R := artifact.NewArtifactSource("hello", ".").\n\tWithIncludes([]string{"src"[I]}).\n\tBuild()\n@T := artifact.NewArtifactStep("bash").\n\tWithScript(@S).\n\tBuild()\n@A, err := artifact.NewArtifact(\n\t"hello",\n\t[]*api.ArtifactStep{@T},\n\tconfig.SYSTEMS,\n).\n\tWithAliases([]string{"hello:[V]"}).\n\tWithSources([]*api.ArtifactSource{&@R}).\n\tBuild(ctx)' },
    { q: '"', inc: ', "docs"', c: '@S := `' + SCRIPT + '`\nincludes := []string{"src"[I]}\naliases := []string{"hello:[V]"}\n\n@R := artifact.NewArtifactSource("hello", ".").\n\tWithIncludes(includes).Build()\n@T := artifact.NewArtifactStep("bash").\n\tWithScript(@S).Build()\n@A, err := artifact.NewArtifact("hello",\n\t[]*api.ArtifactStep{@T}, config.SYSTEMS).\n\tWithSources([]*api.ArtifactSource{&@R}).\n\tWithAliases(aliases).\n\tBuild(ctx)' }
  ],
  rust: [
    { q: '"', inc: ', "docs".into()', c: 'let @S = "' + SCRIPT + '";\nlet @R = ArtifactSource::new("hello", ".")\n    .with_includes(vec!["src".into()[I]])\n    .build();\nlet @T = ArtifactStep::new("bash")\n    .with_script(@S.to_string())\n    .build();\nlet @A = Artifact::new("hello", vec![@T], system::SYSTEMS)\n    .with_aliases(vec!["hello:[V]".into()])\n    .with_sources(vec![@R])\n    .build(&mut ctx)\n    .await?;' },
    { q: '"', inc: ', String::from("docs")', c: 'let @S = String::from("' + SCRIPT + '");\nlet @R = ArtifactSource::new("hello", ".")\n    .with_includes(vec![String::from("src")[I]])\n    .build();\nlet @T = ArtifactStep::new("bash").with_script(@S).build();\nlet @A = Artifact::new("hello", vec![@T], system::SYSTEMS)\n    .with_sources(vec![@R])\n    .with_aliases(vec![String::from("hello:[V]")])\n    .build(&mut ctx)\n    .await?;' }
  ],
  typescript: [
    { q: '"', inc: ', "docs"', c: 'const @S = "' + SCRIPT + '";\nconst @R = new ArtifactSource("hello", ".")\n  .withIncludes(["src"[I]])\n  .build();\nconst @T = new ArtifactStep("bash")\n  .withScript(@S)\n  .build();\nconst @A = await new Artifact("hello", [@T], SYSTEMS)\n  .withAliases(["hello:[V]"])\n  .withSources([@R])\n  .build(ctx);' },
    { q: "'", inc: ", 'docs'", c: "const @S = `" + SCRIPT + "`\nconst @R = new ArtifactSource('hello', '.')\n  .withIncludes(['src'[I]])\n  .build()\nconst @T = new ArtifactStep('bash').withScript(@S).build()\nconst @A = await new Artifact('hello', [@T], SYSTEMS)\n  .withSources([@R])\n  .withAliases(['hello:[V]'])\n  .build(ctx)" }
  ]
};

export function esc(s) { return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;"); }
export function langOf(id) { for (var i = 0; i < 4; i++) if (LANGS[i].id === id) return LANGS[i]; }

/* Tiny highlighter: strings, calls, types, keywords. */
var TOKS = /("(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`[^`]*`)|(\[I\])|([A-Za-z_][A-Za-z0-9_]*)(?=\s*[(!])|([A-Za-z_][A-Za-z0-9_]*)|([^"'`A-Za-z_[]+|[\s\S])/.source;
function hl(line, lang, slot) {
  var kw = " " + KW[lang] + " ", out = "", m, TOK = new RegExp(TOKS, "g");
  while ((m = TOK.exec(line))) {
    if (m[1]) out += '<span class="t-s">' + esc(m[1]).replace("[V]", slot("V")) + "</span>";
    else if (m[2]) out += slot("I");
    else if (m[3]) out += '<span class="' + (/^[A-Z]/.test(m[3]) ? "t-y" : "t-f") + '">' + m[3] + "</span>";
    else if (m[4]) out += kw.indexOf(" " + m[4] + " ") >= 0 ? '<span class="t-k">' + m[4] + "</span>" : /^[A-Z][A-Z]+$/.test(m[4]) ? '<span class="t-y">' + m[4] + "</span>" : m[4];
    else out += esc(m[5]);
  }
  return out;
}

/** Render one language's code. live=false gives plain text slots (no JS view). */
export function codeHtml(lang, st, live) {
  var f = st.f[lang] || 0, t = TPL[lang][f], n = NAMES[lang][st.n[lang] || 0], L = langOf(lang);
  var src = t.c.replace(/@S/g, n[0]).replace(/@R/g, n[1]).replace(/@T/g, n[2]).replace(/@A/g, n[3]);
  function slot(k) {
    if (k === "V") {
      if (!live) return '<span class="slot-s">' + esc(st.ver) + "</span>";
      return '<input class="slot sv" data-slot="ver" type="text" value="' + esc(st.ver) + '" maxlength="20" size="' + Math.max(st.ver.length, 3) + '" spellcheck="false" autocomplete="off" autocapitalize="off" aria-label="Version in the alias, ' + L.name + '" style="width:' + (Math.max(st.ver.length, 3) + 1.4) + 'ch">';
    }
    if (!live) return st.docs ? hl(t.inc, lang, slot) : "";
    var inner = st.docs ? hl(t.inc, lang, slot) : "+ " + '<span class="t-s">' + esc(t.q + "docs" + t.q) + "</span>";
    return '<button type="button" class="slot si" data-slot="inc" aria-pressed="' + st.docs + '"><span class="sr">Include </span><span aria-hidden="true">' + inner + '</span><span class="sr">docs, ' + L.name + "</span></button>";
  }
  return src.split("\n").map(function (l) { return '<span class="ln">' + (hl(l, lang, slot) || " ") + "</span>"; }).join("");
}

export function canon(st) {
  return JSON.stringify({
    target: st.target,
    sources: [{ digest: null, excludes: [], includes: st.docs ? ["src", "docs"] : ["src"], name: "hello", path: "." }],
    steps: [{ entrypoint: "bash", script: SCRIPT, secrets: [], arguments: [], artifacts: [], environments: [] }],
    systems: [1, 2, 3, 4],
    aliases: ["hello:" + st.ver],
    name: "hello"
  });
}
var JT = /("(?:[^"\\]|\\.)*")(:?)|(-?\d+)|(null)|([\s\S])/g;
function jsonHl(s) {
  var o = "", m; JT.lastIndex = 0;
  while ((m = JT.exec(s))) {
    if (m[1]) o += m[2] ? '<span class="j-k">' + esc(m[1]) + "</span>:" : '<span class="t-s">' + esc(m[1]) + "</span>";
    else if (m[3]) o += '<span class="j-n">' + m[3] + "</span>";
    else if (m[4]) o += '<span class="t-k">null</span>';
    else o += esc(m[5]);
  }
  return o;
}
/* Highlighted JSON with <mark> around the fields in `keys`. Text content is exactly canon(st). */
export function canonHtml(st, keys) {
  var s = canon(st), cuts = [];
  (keys || []).forEach(function (k) {
    var a = s.indexOf('"' + k + '":'); if (a < 0) return;
    var v = a + k.length + 3, e = s.charAt(v) === "[" ? s.indexOf("]", v) + 1 : v + /^-?\d+/.exec(s.slice(v))[0].length;
    cuts.push([a, e]);
  });
  cuts.sort(function (x, y) { return x[0] - y[0]; });
  var o = "", p = 0;
  cuts.forEach(function (c) { o += jsonHl(s.slice(p, c[0])) + "<mark>" + jsonHl(s.slice(c[0], c[1])) + "</mark>"; p = c[1]; });
  return o + jsonHl(s.slice(p));
}
export function hexHtml(h) {
  var o = "";
  for (var i = 0; i < 64; i += 16) o += '<span class="hx">' + h.slice(i, i + 16) + "</span>";
  return o;
}
export function bandHtml(h) {
  var o = "";
  for (var i = 0; i < 64; i += 2) o += '<i style="--h:' + Math.round(parseInt(h.substr(i, 2), 16) * 360 / 256) + '"></i>';
  return o;
}

/** The first state: version 1.0.0, no docs, built on x86_64-linux. */
export function initial(lang) { return { ver: "1.0.0", docs: false, target: 4, f: {}, n: {}, lang: lang || "python" }; }
