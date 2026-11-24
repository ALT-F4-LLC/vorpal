// Journey (src/components/Journey.astro): one `vorpal build example` through
// the real parts, ported once from the stage 3 prototype
// (signature-3-journey.html rev 2) as the stage 4 template uses it. The
// prototype's compact style is kept so it can be compared with it.
//
// - D holds the steps, scenarios and the (shortened) definitions that are
//   hashed; the text version in the HTML says the same.
// - Back, Play/Pause (every 6 s, stops at the end), Next, Whole map, a
//   9-stop slider and four "What if" scenarios. One announcement per reader
//   action, to the page's live region (live.ts); autoplay says only
//   "Playing" and "Paused".
// - The camera never shows SVG text below 12 px; reduced motion moves it
//   at once and CSS drops the flows and pulses.
import { say as announce } from './live';

var D={"SCN":{"first":"First build","again":"Build again, nothing changed","edit":"Edit src/main.py","ci":"Second machine or CI"},"STEPS":[{"r":"cli/src/command/build.rs · build_config_binary","t":"Run the build","k":"Run","b":{"first":"In the project `vorpal init` makes for Python, you type `vorpal build example`. The CLI reads `Vorpal.toml`, builds your config program from `src/vorpal.py` (the config program is an artifact too) and starts it.","again":"Same command, nothing edited. The config program is already in the store, so the CLI starts it straight away.","edit":"You changed one line in `src/main.py`. The template’s `Vorpal.toml` lists `src` among the config program’s own sources, so the config program gets a new digest and is rebuilt before it starts.","ci":"A teammate or a CI job on the same system (here `aarch64-linux`) checks out the same commit, `Vorpal.lock` included, and runs the same command. For a pull to work, both machines must use one remote registry (`vorpal config set registry <address>`). By default each machine uses its own local registry."}},{"r":"sdk/python/…/language/python.py · Python.build","t":"Declare the artifacts","k":"Declare","b":{"all":"The config program runs. `Python(\"example\", SYSTEMS)` declares the app, and the Python builder also declares the tools it needs: `cpython` and `uv`. `example`’s build step names both tools, so it depends on them. Each declaration goes to the agent before it gets a digest."}},{"r":"cli/src/command/start/agent.rs · hydrate_and_gate_source","t":"Pin the sources","k":"Pin","b":{"first":"`cpython` and `uv` come from `sdk.vorpal.build`. Their digests are already pinned in `Vorpal.lock` (written earlier by `vorpal prepare`). The agent downloads each archive once, checks it against its pin (a mismatch stops the build unless you pass `--unlock`) and pushes it to the registry. `example`’s source is local: the agent hashes `src/` and the other includes now, after setting every file time to 1970, so only content counts.","again":"The pins in `Vorpal.lock` match and the registry already holds those source archives, so nothing is downloaded. The agent hashes the local `src/` again: same content, same digest.","edit":"The pins are unchanged, so nothing is downloaded. Local sources are not pinned in `Vorpal.lock`: the agent hashes them on every build, and your edit gives `example`’s source a new digest.","ci":"The checkout carries the same `Vorpal.lock`, and the shared registry already holds the pinned source archives, so machine B downloads nothing. Its `src/` hashes to the same digest as on machine A."}},{"r":"cli/src/command/start/agent.rs · prepare_artifact","t":"Give every definition a digest","k":"Digest","b":{"first":"The agent serialises each prepared definition (target, sources with their digests, steps, systems, aliases, name) to JSON and takes its SHA-256. That digest names the artifact. `example`’s step lists the `cpython` and `uv` digests, so a new tool version would change `example` too.","again":"Every input is the same as last time, so every digest is the same as last time.","edit":"Only `example` changes, because its source digest moved. `cpython` and `uv` keep their digests.","ci":"Machine B computes the same three digests as machine A: same definitions, same system."}},{"r":"cli/src/command/build.rs · build","t":"Look in the local store","k":"Store","b":{"first":"For each artifact in dependency order, the CLI checks `/var/lib/vorpal/store/artifact/output/<namespace>/<digest>`. The store is empty, so all three are missing.","again":"All three digests are in the store. Vorpal skips them: no registry call, no build. The rest of this journey is skipped.","edit":"`cpython` and `uv` are found and skipped. The new `example` digest is not there. The old one still is: Vorpal keeps outputs until you run `vorpal system prune`.","ci":"Machine B’s store is empty, so all three are missing locally."}},{"r":"cli/src/command/build.rs · pull_archive","t":"Ask the registry","k":"Registry","b":{"first":"For each miss, the CLI asks the registry (local by default) for `<digest>.tar.zst`. The registry holds the source archives but no built outputs yet, so all three miss.","again":"Skipped. Everything was already in the local store.","edit":"The new `example` digest has never been built anywhere, so the registry misses too.","ci":"This is the remote registry both machines use, and machine A’s builds pushed all three archives here. Machine B pulls them and unpacks them into its own store. Nothing is built."}},{"r":"cli/src/command/start/worker.rs · build_artifact","t":"Build what is missing","k":"Build","b":{"first":"The worker builds each miss in dependency order: `cpython`, `uv`, then `example`. It pulls sources and dependency outputs from the registry into a fresh workspace and runs the steps. Steps run in a sandbox: on Linux, bubblewrap on the `linux-vorpal` root filesystem (macOS sandboxing is planned: ENG-364). It packs the output with zstd and pushes it to the registry.","again":"Idle. There is nothing to build.","edit":"Only `example` is built. The worker pulls the `cpython` and `uv` outputs as they are, runs `example`’s steps in a fresh workspace and pushes the new archive to the registry.","ci":"Idle. Machine B builds nothing."}},{"r":"cli/src/command/build.rs · unpack_archive_if_present","t":"Land in the store","k":"Land","b":{"first":"The CLI pulls each new archive back from the registry, unpacks it into the local store and prints `example`’s digest. Add `--path` to print the output path instead.","again":"Nothing moved. The CLI prints the same digest as last time.","edit":"The CLI pulls the new `example` archive back, unpacks it next to the old one and prints the new digest.","ci":"Already unpacked in the registry step. The CLI prints the same `example` digest that machine A printed."}},{"r":"cli/src/command/run.rs · resolve_alias_digest","t":"Run it","k":"Run it","b":{"all":"`vorpal run example` resolves the alias `example` to a digest (local alias first, then the registry), finds that output (pulling it from the registry if it is missing) and runs its `bin/example`. The alias comes from one line added to the template: `.with_aliases([\"example\"])`.","ci":"`vorpal run example` on machine B resolves the alias and runs `bin/example` from its own store. B could even skip the build: `vorpal run` looks the alias up in the registry and pulls the output itself."}}],"PARCELS":["cpython","uv","example"],"STAND":{"c":"stand-in for the bytes of the cpython archive","u":"stand-in for the bytes of the uv archive","lv":"stand-in for the linux-vorpal definition"},"MAIN":["print(\"Hello, world!\")\n","print(\"Hello, Vorpal!\")\n"],"DEF":{"c":"{\"target\":2,\"sources\":[{\"digest\":\"{pc}\",\"excludes\":[],\"includes\":[],\"name\":\"cpython\",\"path\":\"https://sdk.vorpal.build/source/…\"}],\"steps\":[{\"entrypoint\":\"bwrap\",\"script\":\"… install Python into $VORPAL_OUTPUT\",\"secrets\":[],\"arguments\":[],\"artifacts\":[\"{lv}\"],\"environments\":[]}],\"systems\":[1,2,3,4],\"aliases\":[\"cpython:<version>\"],\"name\":\"cpython\"}","u":"{\"target\":2,\"sources\":[{\"digest\":\"{pu}\",\"excludes\":[],\"includes\":[],\"name\":\"uv\",\"path\":\"https://sdk.vorpal.build/source/…\"}],\"steps\":[{\"entrypoint\":\"bwrap\",\"script\":\"mkdir -p \\\"$VORPAL_OUTPUT/bin\\\" …\",\"secrets\":[],\"arguments\":[],\"artifacts\":[\"{lv}\"],\"environments\":[]}],\"systems\":[1,2,3,4],\"aliases\":[\"uv:<version>\"],\"name\":\"uv\"}","e":"{\"target\":2,\"sources\":[{\"digest\":\"{src}\",\"excludes\":[],\"includes\":[\"pyproject.toml\",\"uv.lock\",\"src\"],\"name\":\"example\",\"path\":\".\"}],\"steps\":[{\"entrypoint\":\"bwrap\",\"script\":\"… uv sync --frozen --no-dev --no-editable …\",\"secrets\":[],\"arguments\":[],\"artifacts\":[\"{lv}\",\"{c}\",\"{u}\"],\"environments\":[\"UV_PYTHON_DOWNLOADS=never\"]}],\"systems\":[1,2,3,4],\"aliases\":[\"example\"],\"name\":\"example\"}"}}, ANCH={"term":[-129.6,74.8],"config":[-2.9,11.9],"agent":[114.8,59.5],"lock":[-2.9,181.9],"worker":[267.9,176.8],"store":[113.4,216.7],"registry":[583,81.6],"termB":[828.9,201.5],"svcB":[920.2,155.5],"storeB":[973.2,250.8]}, BOX={"config":[-242.9,-80.1,76.6,98.6],"agent":[-25.2,-66.5,214.8,164.9],"term":[-379.6,32.3,-55.9,196.8],"registry":[463,-52.4,703,217.6],"worker":[150.2,-24.1,418.5,243.1],"lock":[-102.9,147.9,137.1,303.9],"svcB":[760.2,49.5,1000.2,227.8],"termB":[618.9,164.9,892.2,327.5],"store":[29.4,168.3,383.4,344.8],"storeB":[892.2,204,1263.2,366.8]}, UU=34;
var NS="http://www.w3.org/2000/svg", MINSCALE=12/14; // smallest SVG text is 14 units: keep it at 12 px or more
var RM=window.matchMedia&&matchMedia("(prefers-reduced-motion: reduce)");
function reduced(){return RM&&RM.matches;}
function say(msg){announce(msg);}
var DG={};
var LABEL={"":"not declared yet",decl:"declared",pin:"pin matches Vorpal.lock",hash:"local source hashed",dig:"digest computed",
 hit:"found in store, skipped",misss:"not in store",missr:"not in registry",pull:"pulled from registry",skip:"skipped",
 build:"built in the worker",land:"unpacked into store",in:"in store",run:"running bin/example"};
function code(s){return s.replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/`([^`]+)`/g,"<code>$1</code>");}
function sh(d){return d?d.slice(0,8):"";}
function sha(t){return crypto.subtle.digest("SHA-256",new TextEncoder().encode(t)).then(function(b){
 return Array.prototype.map.call(new Uint8Array(b),function(x){return("0"+x.toString(16)).slice(-2);}).join("");});}
function fill(t,m){return t.replace(/\{(\w+)\}/g,function(_,k){return m[k];});}
var ready=(window.crypto&&crypto.subtle?Promise.all([sha(D.STAND.c),sha(D.STAND.u),sha(D.STAND.lv),sha(D.MAIN[0]),sha(D.MAIN[1])]).then(function(a){
 DG.pc=a[0];DG.pu=a[1];DG.lv=a[2];DG.s0=a[3];DG.s1=a[4];
 DG.dc=fill(D.DEF.c,{pc:DG.pc,lv:DG.lv});DG.du=fill(D.DEF.u,{pu:DG.pu,lv:DG.lv});
 return Promise.all([sha(DG.dc),sha(DG.du)]);}).then(function(a){
 DG.c=a[0];DG.u=a[1];
 DG.de0=fill(D.DEF.e,{src:DG.s0,c:DG.c,u:DG.u,lv:DG.lv});DG.de1=fill(D.DEF.e,{src:DG.s1,c:DG.c,u:DG.u,lv:DG.lv});
 return Promise.all([sha(DG.de0),sha(DG.de1)]);}).then(function(a){DG.e0=a[0];DG.e1=a[1];}):Promise.reject()).catch(function(){DG.none=1;});

function plan(k){
 var ci=k==="ci",T=ci?"termB":"term",CF=ci?"svcB":"config",AG=ci?"svcB":"agent",SB=ci?"storeB":"store";
 var hit=k==="again"?[1,1,1]:k==="edit"?[1,1,0]:[0,0,0],P=[0,1,2],s=[];
 function m(f){return P.map(f);}
 s.push({at:[null,null,null],st:["","",""],hl:[T,CF],ln:[ci?"termB-svcB":"term-config"]});
 s.push({at:m(function(){return CF;}),st:["decl","decl","decl"],hl:[CF],ln:[]});
 s.push({at:m(function(){return AG;}),st:["pin","pin","hash"],hl:ci?[AG,"registry"]:[AG,"lock"].concat(k==="first"?["registry"]:[]),
  ln:ci?["svcB-registry"]:["config-agent","agent-lock"].concat(k==="first"?["agent-registry"]:[])});
 s.push({at:m(function(){return AG;}),st:["dig","dig","dig"],hl:[AG],ln:[]});
 s.push({at:m(function(){return SB;}),st:m(function(i){return hit[i]?"hit":"misss";}),hl:[SB],ln:[ci?"termB-storeB":"term-store"]});
 s.push({at:m(function(i){return hit[i]||ci?SB:"registry";}),st:m(function(i){return hit[i]?"skip":ci?"pull":"missr";}),
  hl:k==="again"?[]:ci?["registry",SB]:["registry"],ln:k==="again"?[]:[ci?"registry-storeB":"store-registry"]});
 var b=m(function(i){return !hit[i]&&!ci;}),any=b.indexOf(true)>-1;
 s.push({at:m(function(i){return b[i]?"worker":SB;}),st:m(function(i){return b[i]?"build":hit[i]?"skip":"in";}),
  hl:any?["worker"]:[],ln:any?["store-worker","worker-registry"]:[]});
 s.push({at:m(function(){return SB;}),st:m(function(i){return b[i]?"land":"in";}),hl:[SB],ln:any?["store-registry"]:[]});
 s.push({at:[SB,SB,T],st:["in","in","run"],hl:[T,SB],ln:[ci?"termB-storeB":"term-store"]});
 return s;
}
function edig(k){return k==="edit"?DG.e1:DG.e0;}
function pdig(i,k){return [DG.c,DG.u,edig(k)][i];}

/** Make the Journey in `root` (section.jr) interactive. */
export function mountJourney(root){if(root.dataset.jrReady)return;root.dataset.jrReady="1";
 var svg=root.querySelector("svg"),scn=root.getAttribute("data-scn")||"first",step=0,timer=null,PL=plan(scn),mode="follow";
 var q=function(c){return root.querySelector(c);};
 var range=q(".jr-range"),prev=q(".jr-prev"),next=q(".jr-next"),play=q(".jr-play"),zoom=q(".jr-zoom");
 var narrH=q(".jr-h"),narrB=q(".jr-body"),cnt=q(".jr-count"),ref=q(".jr-ref"),rows=q(".jr-rows");
 var ticks=root.querySelectorAll(".jr-ticks button"),defs=q(".jr-defs"),strip=q(".jr-strip");
 var vb=svg.getAttribute("data-vb").split(" ").map(Number),cur=vb.slice(),anim=null,sw=0,shh=0;
 var parcels=svg.querySelectorAll(".pc"),stations=svg.querySelectorAll(".st"),tagEls=svg.querySelectorAll(".tag"),links=svg.querySelectorAll(".ln");
 var tagOf={};Array.prototype.forEach.call(tagEls,function(t){var r=t.querySelector("rect");
  tagOf[t.getAttribute("data-t")]={g:t,tx:t.querySelector(".tx"),r:r,tb:t.querySelector(".tb"),ts:t.querySelector(".ts"),x:+r.getAttribute("x")+12,y:+r.getAttribute("y")+62,
   up:t.hasAttribute("data-up"),dr:svg.querySelectorAll('.st[data-s="'+t.getAttribute("data-t")+'"] .drw'),key:""};});
 var regCap=svg.querySelector(".cap-reg");
 function setTagLines(k,lines){
  var o=tagOf[k];if(!o)return;var key=JSON.stringify(lines);if(o.key===key)return;o.key=key;
  var frag=document.createDocumentFragment();
  lines.forEach(function(l,i){var t=document.createElementNS(NS,"text");t.setAttribute("x",o.x);t.setAttribute("y",o.y+i*20);t.setAttribute("class","tl"+(l.c?" "+l.c:""));t.textContent=l.t;frag.appendChild(t);});
  o.tx.textContent="";o.tx.appendChild(frag);
  var extra=lines.length?lines.length*20+6:0;
  o.r.setAttribute("height",52+extra);
  if(o.up)o.tb.style.transform="translateY("+(-extra)+"px)";
  var n=lines.filter(function(l){return l.n;}).length;
  for(var i=0;i<o.dr.length;i++)o.dr[i].classList.toggle("on",i<n);
 }
 function contents(){
  var k=scn,A=[],R=[],B=[],nd=DG.none;
  function it(i,dg,extra,c){return{t:D.PARCELS[i]+(nd?"":" "+sh(dg))+(extra||""),n:1,c:c};}
  var all=[0,1,2].map(function(i){return it(i,pdig(i,k));}),old=it(2,DG.e0," (old)"),nw=it(2,DG.e1," (new)","new");
  if(k==="again"||k==="ci"){A=all;R=all;}
  if(k==="edit"){A=[all[0],all[1],old];R=A.slice();if(step>=6)R=R.concat([nw]);if(step>=7)A=A.concat([nw]);}
  if(k==="first"){if(step>=6)R=all;if(step>=7)A=all;}
  if(k==="ci"&&step>=5)B=all;
  if(!R.length)R=[{t:step>=2?"source archives only":"empty",c:"dim"}];
  if(!A.length)A=[{t:"empty",c:"dim"}];
  if(!B.length)B=[{t:"empty",c:"dim"}];
  setTagLines("store",A);setTagLines("registry",R);setTagLines("storeB",B);
  var T=k==="ci"?"termB":"term",O=k==="ci"?"term":"termB",tl=[{t:"$ vorpal build example"}];
  if(step>=8)tl=tl.concat([{t:"$ vorpal run example"},{t:k==="edit"?"Hello, Vorpal!":"Hello, world!",c:"out"}]);
  setTagLines(T,tl);setTagLines(O,[]);
  var st=PL[step];
  setTagLines("worker",st.hl.indexOf("worker")>-1?[0,1,2].filter(function(i){return st.st[i]==="build";}).map(function(i){return{t:"building "+D.PARCELS[i],c:"new"};}):step===6?[{t:"idle",c:"dim"}]:[]);
  setTagLines("lock",step===2&&k!=="ci"?[{t:"cpython pin matches"},{t:"uv pin matches"}]:[]);
  tagOf.registry.ts.textContent=k==="ci"?"remote, shared by A and B":"local by default";
  if(regCap)regCap.textContent=k==="ci"?"SHARED REGISTRY":"REGISTRY";
 }
 // camera: never shows SVG text below 12 px; sizes come from the ResizeObserver (no forced layout)
 function camera(instant){
  if(!sw)return;
  var maxW=sw/MINSCALE,want;
  if(mode==="map"||vb[2]<=maxW){want=vb.slice();}
  else{
   var hl=step===8?[scn==="ci"?"termB":"term"]:PL[step].hl.length?PL[step].hl:[scn==="ci"?"storeB":"store"];
   function bb(list){var x0=1e9,y0=1e9,x1=-1e9,y1=-1e9;list.forEach(function(k){var b=BOX[k];x0=Math.min(x0,b[0]);y0=Math.min(y0,b[1]);x1=Math.max(x1,b[2]);y1=Math.max(y1,b[3]);});return[x0-30,y0-80,x1+40,y1+30];}
   var b=bb(hl);if(b[2]-b[0]>maxW)b=bb([hl[0]]);
   var ar=sw/shh,w=Math.min(maxW,Math.max(b[2]-b[0],maxW*.9)),h=w/ar;
   if(b[3]-b[1]>h){h=Math.min(b[3]-b[1],maxW/ar);w=h*ar;}
   var cx=(b[0]+b[2])/2,cy=(b[1]+b[3])/2;want=[cx-w/2,cy-h/2,w,h];
  }
  var small=vb[2]>maxW;root.classList.toggle("cam",small&&mode!=="map");svg.classList.toggle("map-small",mode==="map"&&small);
  if(anim)cancelAnimationFrame(anim);
  if(instant||reduced()){cur=want;svg.setAttribute("viewBox",cur.map(function(v){return v.toFixed(1);}).join(" "));return;}
  var from=cur.slice(),t0=performance.now();
  (function tick(t){var p=Math.min(1,(t-t0)/600),e=p<.5?2*p*p:1-Math.pow(-2*p+2,2)/2;
   cur=from.map(function(v,i){return v+(want[i]-v)*e;});svg.setAttribute("viewBox",cur.map(function(v){return v.toFixed(1);}).join(" "));
   if(p<1)anim=requestAnimationFrame(tick);})(t0);
 }
 function statusText(){var st=PL[step];return D.PARCELS.map(function(n,i){return n+": "+LABEL[st.st[i]];}).join("; ")+".";}
 function setDis(b,d){b.setAttribute("aria-disabled",d?"true":"false");}
 function render(){
  var st=PL[step];
  range.value=step;range.setAttribute("aria-valuetext","Step "+(step+1)+" of 9: "+D.STEPS[step].t);
  setDis(prev,step===0);setDis(next,step===8);
  for(var i=0;i<ticks.length;i++){ticks[i].classList.toggle("on",i===step);ticks[i].classList.toggle("done",i<step);}
  cnt.textContent="Step "+(step+1)+" of 9 · "+D.SCN[scn];
  narrH.textContent=D.STEPS[step].t;
  var b=D.STEPS[step].b;narrB.innerHTML=code(b[scn]||b.all);ref.textContent="In the code: "+D.STEPS[step].r;
  rows.innerHTML=D.PARCELS.map(function(n,i){var c=st.st[i]||"none",dg=step>=3?pdig(i,scn):null;
   return '<li class="r-'+c+'"><span class="nm">'+n+'</span><span class="dg">'+(dg?sh(dg):"")+'</span><span class="chip">'+LABEL[st.st[i]]+'</span></li>';}).join("");
  svg.classList.toggle("focus",st.hl.length>0);
  Array.prototype.forEach.call(stations,function(e){e.classList.toggle("on",st.hl.indexOf(e.getAttribute("data-s"))>-1);});
  Array.prototype.forEach.call(tagEls,function(e){e.classList.toggle("on",st.hl.indexOf(e.getAttribute("data-t"))>-1);});
  Array.prototype.forEach.call(links,function(e){e.classList.toggle("on",st.ln.indexOf(e.getAttribute("data-l"))>-1);});
  var lvl={};
  Array.prototype.forEach.call(parcels,function(g,i){var at=st.at[i];
   g.setAttribute("class","pc p"+i+" s-"+(st.st[i]||"none")+(at?"":" gone"));
   var key=at||(scn==="ci"?"termB":"term"),a=ANCH[key],L=lvl[key]=(lvl[key]||0);lvl[key]++;
   g.style.transform="translate("+a[0]+"px,"+(a[1]-L*0.8*UU)+"px)";
   g.querySelector("text").textContent=D.PARCELS[i]+(step>=3&&!DG.none&&DG.c?" · "+sh(pdig(i,scn)):"");
  });
  contents();camera();
 }
 function digestsUI(){
  if(DG.none){strip.textContent="Your browser could not compute SHA-256 here, so no digests are shown.";return;}
  var e=edig(scn);
  strip.innerHTML='<span><b>cpython</b> <code>'+sh(DG.c)+'</code></span><span><b>uv</b> <code>'+sh(DG.u)+'</code></span><span><b>example</b> <code>'+sh(e)+'</code>'+
   (scn==="edit"?' <em class="chg">changed, was <code>'+sh(DG.e0)+'</code></em>':' <em>unchanged</em>')+'</span>';
  defs.innerHTML='<p class="dl">src/main.py</p><pre>'+code(D.MAIN[scn==="edit"?1:0])+'</pre>'+
   [["cpython",DG.dc,DG.c],["uv",DG.du,DG.u],["example",scn==="edit"?DG.de1:DG.de0,e]].map(function(r){
   return '<p class="dl">'+r[0]+'</p><pre>'+code(r[1])+'</pre><p class="sha">SHA-256 <code>'+r[2]+'</code></p>';}).join("")+
   '<p class="fine">Real serializer shape and field order (target, sources, steps, systems, aliases, name; enums as integers). Values are shortened: scripts are cut, the Linux <code>bwrap</code> arguments are left out, pins and the <code>linux-vorpal</code> digest are SHA-256 of stand-in strings, and <code>example</code>’s source digest is SHA-256 of <code>src/main.py</code> alone.</p>';
 }
 // user moves: one announcement each. Slider already speaks its valuetext, so the live region adds only statuses.
 function go(n,how){
  n=Math.max(0,Math.min(8,n));if(n===step&&how!=="scn")return;step=n;render();
  if(how==="auto")return;
  if(how==="slider")say(statusText());
  else if(how==="scn")say("What if: "+D.SCN[scn]+". Step "+(step+1)+" of 9, "+D.STEPS[step].t+". "+statusText());
  else say("Step "+(step+1)+" of 9, "+D.STEPS[step].t+". "+statusText());
 }
 function stop(silent){if(!timer)return;clearInterval(timer);timer=null;play.textContent="Play";
  if(!silent)say("Paused at step "+(step+1)+" of 9, "+D.STEPS[step].t+".");}
 prev.addEventListener("click",function(){if(step===0)return;stop(true);go(step-1);});
 next.addEventListener("click",function(){if(step===8)return;stop(true);go(step+1);});
 range.addEventListener("input",function(){stop(true);go(+range.value,"slider");});
 Array.prototype.forEach.call(ticks,function(b,i){b.addEventListener("click",function(){stop(true);go(i);});});
 play.addEventListener("click",function(){
  if(timer){stop();return;}
  if(step===8)go(0,"auto");
  play.textContent="Pause";say("Playing from step "+(step+1)+". Steps advance every 6 seconds.");
  timer=setInterval(function(){go(step+1,"auto");if(step>=8)stop();},6000);
 });
 zoom.addEventListener("click",function(){mode=mode==="follow"?"map":"follow";zoom.setAttribute("aria-pressed",mode==="map"?"true":"false");camera();});
 Array.prototype.forEach.call(root.querySelectorAll(".jr-scn input"),function(r){
  r.checked=r.value===scn;
  r.addEventListener("change",function(){stop(true);scn=r.value;root.setAttribute("data-scn",scn);PL=plan(scn);digestsUI();go(step,"scn");});
 });
 if(window.ResizeObserver)new ResizeObserver(function(en){var c=en[0].contentRect;var first=!sw;sw=c.width;shh=c.height;camera(first);}).observe(svg);
 render();
 ready.then(function(){digestsUI();render();});
}
