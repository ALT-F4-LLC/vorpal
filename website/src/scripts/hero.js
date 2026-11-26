// The Home hero's scene (src/components/home/Hero.astro): ported once from
// the approved prototype, docs-redesign stage3/home-hero.html rev 10 (stage 4
// disposition M4). The code keeps the prototype's compact style, so a diff
// against the prototype stays readable.
//
// What it does:
// - Upgrades the picked tiles to buttons with a popover (Runs on, TLS, OIDC,
//   edit src/main.py, bump uv in Vorpal.lock), and the CLI users to tiles
//   with an access badge.
// - Draws the routes: an orthogonal path search on a 4 px grid around every
//   box (so no route crosses a box), badges centred on their routes, and
//   nothing between machines while the registry is on the laptop's socket.
// - Moves services between stages: changed routes retract, tiles glide, and
//   new routes draw only when every glide has ended.
// - Builds: the agent daemon pushes sources to the registry; the CLI sends
//   the worker a build request and gets its logs; the worker pulls sources
//   from the registry and pushes archives back; the CLI and other CLI users
//   pull archives from the registry. Digests are SHA-256 of the Python SDK's
//   definitions, computed in the browser.
// - Autoplay loops stage 1, 2, 3 with a Pause control; it pauses while the
//   scene is off screen or the tab is hidden. Reduced motion: no autoplay,
//   and every change is instant.
// - Announcements go to the page's one live region (src/scripts/live.ts),
//   and only after the reader has pressed a key or pointed: autoplay updates
//   the visible caption only.
import { say as announce } from './live';

var $$=function(s,r){return [].slice.call((r||document).querySelectorAll(s));};
var heroUser=false;["pointerdown","keydown"].forEach(function(ev){document.addEventListener(ev,function(){heroUser=true;},true);});
function say(t){if(heroUser)announce(t);}
var RM=matchMedia("(prefers-reduced-motion: reduce)");
var enc=new TextEncoder();
function sha(s){return crypto.subtle.digest("SHA-256",enc.encode(s)).then(function(b){
 return Array.prototype.map.call(new Uint8Array(b),function(x){return ("0"+x.toString(16)).slice(-2);}).join("");});}
/* Definitions emitted by the Python SDK builders (stub context, aarch64-darwin). @@D:x@@ = digest of x, @@S:x@@ = source digest of x. */
var TPL={"cpython":"{\"target\":1,\"sources\":[{\"digest\":\"@@S:cpython@@\",\"excludes\":[],\"includes\":[],\"name\":\"cpython\",\"path\":\"https://sdk.vorpal.build/source/cpython-3.14.7-aarch64-apple-darwin.tar.gz\"}],\"steps\":[{\"entrypoint\":\"bash\",\"script\":\"#!/bin/bash\\nset -euo pipefail\\n\\nmkdir -p \\\"$VORPAL_OUTPUT\\\"\\ncp -prf \\\"./source/cpython/python/.\\\" \\\"$VORPAL_OUTPUT/\\\"\\n\\n\",\"secrets\":[],\"arguments\":[],\"artifacts\":[],\"environments\":[\"HOME=$VORPAL_WORKSPACE\",\"PATH=:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin\"]}],\"systems\":[1,2,3,4],\"aliases\":[\"cpython:3.14.7\"],\"name\":\"cpython\"}","uv":"{\"target\":1,\"sources\":[{\"digest\":\"@@S:uv@@\",\"excludes\":[],\"includes\":[],\"name\":\"uv\",\"path\":\"https://sdk.vorpal.build/source/uv-@@UVV@@-aarch64-apple-darwin.tar.gz\"}],\"steps\":[{\"entrypoint\":\"bash\",\"script\":\"#!/bin/bash\\nset -euo pipefail\\n\\nmkdir -p \\\"$VORPAL_OUTPUT/bin\\\"\\ncp -p \\\"./source/uv/uv-aarch64-apple-darwin/uv\\\" \\\"$VORPAL_OUTPUT/bin/uv\\\"\\nchmod +x \\\"$VORPAL_OUTPUT/bin/uv\\\"\\n\\n\",\"secrets\":[],\"arguments\":[],\"artifacts\":[],\"environments\":[\"HOME=$VORPAL_WORKSPACE\",\"PATH=:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin\"]}],\"systems\":[1,2,3,4],\"aliases\":[\"uv:@@UVV@@\"],\"name\":\"uv\"}","greeter-dev":"{\"target\":1,\"sources\":[],\"steps\":[{\"entrypoint\":\"bash\",\"script\":\"#!/bin/bash\\nset -euo pipefail\\n\\nmkdir -p $VORPAL_WORKSPACE/bin\\n\\ncat > bin/activate << \\\"EOF\\\"\\n#!/bin/bash\\n\\nexport VORPAL_SHELL_BACKUP_PATH=\\\"$PATH\\\"\\nexport VORPAL_SHELL_BACKUP_PS1=\\\"$PS1\\\"\\nexport VORPAL_SHELL_BACKUP_VORPAL_SHELL=\\\"$VORPAL_SHELL\\\"\\nexport VORPAL_SHELL_BACKUP_UV_PYTHON=\\\"$UV_PYTHON\\\"\\nexport VORPAL_SHELL_BACKUP_UV_PYTHON_DOWNLOADS=\\\"$UV_PYTHON_DOWNLOADS\\\"\\nexport PS1=\\\"(greeter-dev) $PS1\\\"\\nexport VORPAL_SHELL=\\\"1\\\"\\nexport UV_PYTHON=$VORPAL_ARTIFACT_@@D:cpython@@/bin/python3\\nexport UV_PYTHON_DOWNLOADS=never\\nexport PATH=$VORPAL_ARTIFACT_@@D:cpython@@/bin:$VORPAL_ARTIFACT_@@D:uv@@/bin:$PATH\\n\\ndeactivate(){\\nexport PATH=\\\"$VORPAL_SHELL_BACKUP_PATH\\\"\\nexport PS1=\\\"$VORPAL_SHELL_BACKUP_PS1\\\"\\nexport VORPAL_SHELL=\\\"$VORPAL_SHELL_BACKUP_VORPAL_SHELL\\\"\\nexport UV_PYTHON=\\\"$VORPAL_SHELL_BACKUP_UV_PYTHON\\\"\\nexport UV_PYTHON_DOWNLOADS=\\\"$VORPAL_SHELL_BACKUP_UV_PYTHON_DOWNLOADS\\\"\\nunset VORPAL_SHELL_BACKUP_PATH\\nunset VORPAL_SHELL_BACKUP_PS1\\nunset VORPAL_SHELL_BACKUP_VORPAL_SHELL\\nunset VORPAL_SHELL_BACKUP_UV_PYTHON\\nunset VORPAL_SHELL_BACKUP_UV_PYTHON_DOWNLOADS\\n}\\n\\nexec \\\"$@\\\"\\nEOF\\n\\nchmod +x $VORPAL_WORKSPACE/bin/activate\\n\\nmkdir -p $VORPAL_OUTPUT/bin\\n\\ncp -pr bin \\\"$VORPAL_OUTPUT\\\"\\n\",\"secrets\":[],\"arguments\":[],\"artifacts\":[\"@@D:cpython@@\",\"@@D:uv@@\"],\"environments\":[\"HOME=$VORPAL_WORKSPACE\",\"PATH=$VORPAL_ARTIFACT_@@D:cpython@@/bin:$VORPAL_ARTIFACT_@@D:uv@@/bin:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin\"]}],\"systems\":[1,2,3,4],\"aliases\":[],\"name\":\"greeter-dev\"}","greeter":"{\"target\":1,\"sources\":[{\"digest\":\"@@S:greeter@@\",\"excludes\":[],\"includes\":[\"pyproject.toml\",\"uv.lock\",\"src\"],\"name\":\"greeter\",\"path\":\".\"}],\"steps\":[{\"entrypoint\":\"bash\",\"script\":\"#!/bin/bash\\nset -euo pipefail\\n\\npushd ./source/greeter\\n\\n\\n\\nuv sync --frozen --no-dev --no-editable\\n\\ncp -pr . \\\"$VORPAL_OUTPUT/\\\"\\n\\nmkdir -p \\\"$VORPAL_OUTPUT/bin\\\"\\n\\ncat > \\\"$VORPAL_OUTPUT/bin/greeter\\\" << EOF\\n#!/usr/bin/env bash\\nset -euo pipefail\\nVORPAL_PYTHON_ROOT=\\\"\\\\$(cd \\\"\\\\$(dirname \\\"\\\\${BASH_SOURCE[0]}\\\")/..\\\" && pwd)\\\"\\nPYTHONPATH_EXTRA=\\\"\\\\$VORPAL_PYTHON_ROOT\\\"\\nfor site in \\\"\\\\$VORPAL_PYTHON_ROOT\\\"/.venv/lib/python*/site-packages; do\\n    [ -d \\\"\\\\$site\\\" ] && PYTHONPATH_EXTRA=\\\"\\\\$site:\\\\$PYTHONPATH_EXTRA\\\"\\ndone\\nexport PYTHONPATH=\\\"\\\\$PYTHONPATH_EXTRA\\\\${PYTHONPATH:+:\\\\$PYTHONPATH}\\\"\\nexec \\\"$VORPAL_ARTIFACT_@@D:cpython@@/bin/python3\\\" \\\"\\\\$VORPAL_PYTHON_ROOT/src/main.py\\\" \\\"\\\\$@\\\"\\nEOF\\n\\nchmod +x \\\"$VORPAL_OUTPUT/bin/greeter\\\"\\n\",\"secrets\":[],\"arguments\":[],\"artifacts\":[\"@@D:cpython@@\",\"@@D:uv@@\"],\"environments\":[\"UV_PYTHON=$VORPAL_ARTIFACT_@@D:cpython@@/bin/python3\",\"UV_PYTHON_DOWNLOADS=never\",\"UV_LINK_MODE=copy\",\"UV_CACHE_DIR=$VORPAL_WORKSPACE/uv/cache\",\"SOURCE_DATE_EPOCH=315532800\",\"HOME=$VORPAL_WORKSPACE\",\"PATH=$VORPAL_ARTIFACT_@@D:uv@@/bin:$VORPAL_ARTIFACT_@@D:cpython@@/bin:$VORPAL_ARTIFACT_@@D:cpython@@/bin:$VORPAL_ARTIFACT_@@D:uv@@/bin:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin\"]}],\"systems\":[1,2,3,4],\"aliases\":[],\"name\":\"greeter\"}","greeter-test":"{\"target\":1,\"sources\":[],\"steps\":[{\"entrypoint\":\"bash\",\"script\":\"#!/bin/bash\\nset -euo pipefail\\n\\n@@JOB@@\\n\",\"secrets\":[],\"arguments\":[],\"artifacts\":[\"@@D:greeter@@\"],\"environments\":[\"HOME=$VORPAL_WORKSPACE\",\"PATH=$VORPAL_ARTIFACT_@@D:greeter@@/bin:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin\"]}],\"systems\":[1,2,3,4],\"aliases\":[],\"name\":\"greeter-test\"}"};
var ARTS=["cpython","uv","greeter","greeter-dev","greeter-test"],SRCS=["cpython","uv","greeter"],PULL=["cpython","uv","greeter"];
var SRC="https://sdk.vorpal.build/source/",TRIPLE="aarch64-apple-darwin";
function jstr(s){return JSON.stringify(s).slice(1,-1);}
function define(inp){
 var D={},S={},out={a:{},s:{}};
 function fill(n){return TPL[n].replace(/@@(D|S):([a-z-]+)@@|@@UVV@@|@@JOB@@/g,function(m,k,x){
  return k==="D"?D[x]:k==="S"?S[x]:m==="@@UVV@@"?jstr(inp.uv):jstr(inp.job);});}
 return Promise.all([sha(SRC+"cpython-3.14.7-"+TRIPLE+".tar.gz"),sha(SRC+"uv-"+inp.uv+"-"+TRIPLE+".tar.gz"),sha(inp.main+"\n")]).then(function(p){
  S.cpython=p[0];S.uv=p[1];S.greeter=p[2];out.s=S;
  return ARTS.reduce(function(c,n){return c.then(function(){var t=fill(n);return sha(t).then(function(d){D[n]=d;out.a[n]=d;});});},Promise.resolve());
 }).then(function(){return out;});
}
function list(a){return a.length<2?a.join(""):a.slice(0,-1).join(", ")+" and "+a[a.length-1];}
function esc(s){return String(s).replace(/[&<>"]/g,function(c){return {"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c];});}
var HEX="0123456789abcdef";
var CONS0=[{id:"c3",name:"AI agent",icon:"hxi-bot",kind:"human",access:"read"},{id:"c1",name:"teammate",icon:"hxi-users",kind:"human",access:"read"},{id:"c2",name:"CI",icon:"hxi-ci",kind:"service",access:"read"}];
var CONS_ADD=[{id:"c4",name:"contractor",icon:"hxi-users",kind:"human",access:"none"},{id:"c5",name:"staging",icon:"hxi-box",kind:"service",access:"read"}];
var NS="http://www.w3.org/2000/svg";

/** Start the scene in `sc` ([data-hx]); `user` is true when a reader action loaded it. */
export function mount(sc,user){if(sc.dataset.hxReady)return;sc.dataset.hxReady="1";if(user)heroUser=true;
 var flow=sc.querySelector(".flow"),svg=sc.querySelector(".links"),base=svg.querySelector(".base"),fx=svg.querySelector(".fx"),btn=sc.querySelector('[data-act="build"]');
 function q(s){return sc.querySelector(s);}
 var S={cur:null,reg:new Set(),store:new Set(),lock:"0.13.8",uv:"0.13.8",main:'print("Hello, world!")',tls:true,oidc:true,
  where:{registry:"laptop",w1:"laptop"},workers:["w1"],buildOn:"w1",cons:[],added:0,busy:false,skip:false,once:false,note:{}};
 var T={},tiles={},links=[];
 /* timing: every wait and animation can be skipped at once */
 var D={on:false,paused:false,susp:false,vis:true,loop:false,n:0,told:{}};
 var pend=[],anims=new Set(),held=[];
 function frozen(){return D.paused||D.susp;}
 function quick(){return RM.matches||S.skip;}
 function tick(ms){return new Promise(function(r){var o={r:r};o.t=setTimeout(function(){var i=pend.indexOf(o);if(i>=0)pend.splice(i,1);if(frozen())held.push(r);else r();},ms);pend.push(o);});}
 var SP=1.3;
 function wait(ms){if(quick())return Promise.resolve();return tick(ms*SP);}
 function hold(ms){return tick(ms);}
 function track(a){anims.add(a);if(frozen())a.pause();var f=function(){anims.delete(a);};a.finished.then(f,f);return a;}
 function flush(){pend.splice(0).forEach(function(o){clearTimeout(o.t);o.r();});held.splice(0).forEach(function(r){r();});anims.forEach(function(a){try{a.finish();}catch(e){}});}
 function skipAll(){S.skip=true;flush();}
 function sync(){var f=frozen();anims.forEach(function(a){try{if(f)a.pause();else if(a.playState==="paused")a.play();}catch(e){}});if(!f)held.splice(0).forEach(function(r){r();});}
 function scramble(el,pre,target,dur){if(quick()){el.innerHTML=pre+'<span class="dg">'+target+"</span>";return;}
  el.innerHTML=pre+'<span class="dg"></span>';var sp=el.lastChild,t0=null;
  function f(t){if(t0===null)t0=t;var k=quick()?1:(t-t0)/((dur||360)*SP),n=Math.min(target.length,Math.floor(k*target.length)),s=target.slice(0,n);
   for(var i=n;i<target.length;i++)s+=HEX[(Math.random()*16)|0];sp.textContent=k>=1?target:s;if(k<1)requestAnimationFrame(f);}
  requestAnimationFrame(f);}

 function upgrade(d){var b=document.createElement("button");b.type="button";
  for(var i=0;i<d.attributes.length;i++){var a=d.attributes[i];b.setAttribute(a.name,a.value);}
  while(d.firstChild)b.appendChild(d.firstChild);d.parentNode.replaceChild(b,d);return b;}
 $$("[data-pick]",sc).forEach(function(d){var b=upgrade(d);T[b.dataset.id]=b;b.setAttribute("aria-haspopup","dialog");b.setAttribute("aria-expanded","false");});
 tiles.laptop=q(".machine");tiles.cli=q('[data-tile="cli"]');tiles.agent=q('[data-tile="agent"]');tiles.registry=q('[data-tile="registry"]');
 tiles.main=T.main;tiles.lock=T.lock;tiles.w1=T.w1;
 $$("[data-static]",sc).forEach(function(d){d.parentNode.removeChild(d);});
 function consEl(c){var d=document.createElement("div");d.className="tile cons";d.dataset.tile=c.id;
  d.innerHTML='<button type="button" class="pull" data-id="'+c.id+'"><svg class="ic" aria-hidden="true"><use href="#'+c.icon+'"/></svg><span class="t">'+esc(c.name)+'</span><span class="m" data-m="'+c.id+'"></span></button><span class="idw" data-idw="'+c.id+'"></span>';
  q("[data-cons]").appendChild(d);tiles[c.id]=d;T[c.id]=d.querySelector(".pull");
  T[c.id].addEventListener("click",function(){if(D.on)stopDemo().then(function(){pull(c.id);});else pull(c.id);});}
 function addCons(c){c=Object.assign({has:null,err:null},c);S.cons.push(c);consEl(c);return c;}
 CONS0.forEach(addCons);

 function sh(k){return q('.sh[data-k="'+k+'"]');}
 function cons(id){for(var i=0;i<S.cons.length;i++)if(S.cons[i].id===id)return S.cons[i];}
 function wname(w){return w==="w1"?"Worker":"Worker 2";}
 function whereOf(id){if(id==="laptop"||id==="cli"||id==="agent")return "laptop";if(S.where[id])return S.where[id];return "cons";}
 function reach(a,b){return !(S.where.registry==="laptop"&&(a==="registry"||b==="registry")&&whereOf(a==="registry"?b:a)!=="laptop");}
 function allowed(c){return !S.oidc||c.kind==="service"||c.access==="read";}
 function m(id){return q('[data-m="'+id+'"]');}
 function tile(id){return tiles[id];}
 function wstep(w,k){var t=tile(w);if(!t)return;$$(".stp",t).forEach(function(e,i){e.dataset.s=k===0?"":i+1<k?"done":i+1===k?"run":"";});}
 function wnow(w,h){var e=q('[data-now="'+w+'"]');if(e)e.innerHTML=h;}
 function st(id,cls){var t=tile(id);if(!t)return;["work","live","err","off"].forEach(function(k){t.classList.toggle(k,k===cls);});}

 /* ---------- paths between machines ---------- */
 /* Routes. Kinds: local (inside the laptop), net (network, lock badge), cons (to a CLI user, lock + role badge),
    stub (unreachable: a short dotted stub with a ban cap). With the registry on the laptop, nothing outside the laptop gets a route. */
 function linkDefs(){var RL=S.where.registry==="laptop",L=[["cli","agent","local"]];
  if(RL){L.push(["agent","registry","local"]);L.push(["cli","registry","local"]);}else L.push(["laptop","registry","net","agent"]);
  S.workers.forEach(function(w){var WL=S.where[w]==="laptop";
   if(w===S.buildOn)L.push(WL?["cli",w,"local"]:["laptop",w,"net","cli"]);
   L.push([w,"registry",WL&&RL?"local":RL?"stub":"net"]);});
  if(!RL)S.cons.forEach(function(c){L.push(["registry",c.id,"cons"]);});return L;}
 var ACT={},RT={key:"",geo:null};
 function box(e,g){var r=e.getBoundingClientRect();return {l:r.left-g.left,t:r.top-g.top,r:r.right-g.left,b:r.bottom-g.top};}
 function hit(a,b,p){return a.l-p<b.r&&b.l-p<a.r&&a.t-p<b.b&&b.t-p<a.b;}
 function obstacles(g){var o=[];$$(".tile,.ghost,.add",flow).forEach(function(e){if(e.hidden||!e.offsetParent)return;o.push(box(e,g));});
  $$(".zl h2",flow).forEach(function(h){var r=document.createRange();r.selectNodeContents(h);var b=r.getBoundingClientRect();o.push({l:b.left-g.left,t:b.top-g.top,r:b.right-g.left,b:b.bottom-g.top});});return o;}
 /* a small heap for the path search */
 function Heap(){this.k=[];this.v=[];}
 Heap.prototype.push=function(k,v){var K=this.k,V=this.v,i=K.length;K.push(k);V.push(v);while(i>0){var p=(i-1)>>1;if(K[p]<=k)break;K[i]=K[p];V[i]=V[p];i=p;}K[i]=k;V[i]=v;};
 Heap.prototype.pop=function(){var K=this.k,V=this.v,top=V[0],k=K.pop(),v=V.pop(),n=K.length;if(n){var i=0;for(;;){var l=2*i+1,r=l+1,m=i,mk=k;if(l<n&&K[l]<mk){m=l;mk=K[l];}if(r<n&&K[r]<mk)m=r;if(m===i)break;K[i]=K[m];V[i]=V[m];i=m;}K[i]=k;V[i]=v;}return top;};
 var DX=[1,0,-1,0],DY=[0,1,0,-1];
 /* orthogonal search on a 4px grid: boxes are walls, routes keep 12px straight off each box edge, prefer gutters and their own lane */
 function search(A,B,obs,G,use,sa,sb,sl){var c=G.c,W=G.W,H=G.H,N=W*H,blk=new Uint8Array(N),near=new Uint8Array(N),STUB=12;
  function fill(o,inf,arr){var x0=Math.max(0,Math.ceil((o.l-inf-G.x0)/c-.5)),x1=Math.min(W-1,Math.floor((o.r+inf-G.x0)/c-.5)),y0=Math.max(0,Math.ceil((o.t-inf-G.y0)/c-.5)),y1=Math.min(H-1,Math.floor((o.b+inf-G.y0)/c-.5));
   for(var y=y0;y<=y1;y++)for(var x=x0;x<=x1;x++)arr[y*W+x]=1;}
  obs.forEach(function(o){if(o===A||o===B)return;fill(o,10,near);});obs.forEach(function(o){if(o===A||o===B)return;fill(o,c<4?5:6,blk);});
  fill(A,0,blk);fill(B,0,blk);
  function cell(x,y){var gx=Math.floor((x-G.x0)/c),gy=Math.floor((y-G.y0)/c);return gx<0||gy<0||gx>=W||gy>=H?-1:gy*W+gx;}
  function cx(i){return G.x0+(i%W+.5)*c;}function cy(i){return G.y0+(Math.floor(i/W)+.5)*c;}
  function ports(R,fn,only,SL){SL=SL||STUB;[[0,R.r,(R.t+R.b)/2,R.t,R.b],[1,(R.l+R.r)/2,R.b,R.l,R.r],[2,R.l,(R.t+R.b)/2,R.t,R.b],[3,(R.l+R.r)/2,R.t,R.l,R.r]].forEach(function(s){
   var d=s[0];if(only!=null&&only!==d)return;var hz=d===0||d===2,lo=s[3]+10,hi=s[4]-10,mid=(lo+hi)/2;
   for(var p=lo;p<=hi;p+=c){var ex=hz?s[1]:p,ey=hz?p:s[2],x=ex+DX[d]*SL,y=ey+DY[d]*SL,i=cell(x,y);if(i<0||blk[i])continue;
    var ok=true;for(var t=4;t<SL;t+=4){var j=cell(ex+DX[d]*t,ey+DY[d]*t);if(j<0||blk[j])ok=false;}
    if(!ok)continue;fn(i,d,hz?{x:ex,y:cy(i)}:{x:cx(i),y:ey},Math.abs(p-mid)*.08);}});}
  var g=new Float32Array(N*4).fill(1e9),par=new Int32Array(N*4).fill(-1),hp=new Heap(),tg=new Map(),st=new Map();
  function h(i){var x=cx(i),y=cy(i),Z=sl||STUB;return (Math.max(0,B.l-Z-x,x-B.r-Z)+Math.max(0,B.t-Z-y,y-B.b-Z))/c;}
  ports(B,function(i,d,e,pen){var o=tg.get(i);if(!o||o.pen>pen)tg.set(i,{d:(d+2)%4,e:e,pen:pen});},sb,sl);
  ports(A,function(i,d,e,pen){var s=i*4+d;if(pen<g[s]){g[s]=pen;st.set(s,e);hp.push(pen+h(i),s);}},sa);
  var best=1e9,bs=-1,TURN=3;
  while(hp.k.length){var f=hp.k[0],s=hp.pop();if(f>=best)break;var i=s>>2,d=s&3,gs=g[s];if(f-h(i)>gs+1e-3)continue;
   var t=tg.get(i);if(t){var tot=gs+(d===t.d?0:TURN)+t.pen;if(tot<best){best=tot;bs=s;}}
   var x=i%W,y=(i-x)/W;
   for(var nd=0;nd<4;nd++){if(nd===(d+2)%4)continue;var nx=x+DX[nd],ny=y+DY[nd];if(nx<0||ny<0||nx>=W||ny>=H)continue;var j=ny*W+nx;if(blk[j])continue;
    var ng=gs+1+(nd!==d?TURN:0)+near[j]*.6+use[j],ns=j*4+nd;if(ng<g[ns]){g[ns]=ng;par[ns]=s;hp.push(ng+h(j),ns);}}}
  if(bs<0)return null;
  var cells=[],s2=bs;while(s2>=0){cells.push(s2);if(par[s2]<0)break;s2=par[s2];}cells.reverse();
  var p=[st.get(cells[0])];cells.forEach(function(s){p.push({x:cx(s>>2),y:cy(s>>2)});});p.push(tg.get(bs>>2).e);
  cells.forEach(function(s){var i=s>>2,x=i%W,y=(i-x)/W;var rr=c<4?4:2;for(var a=-rr;a<=rr;a++)for(var b=-rr;b<=rr;b++){var X=x+a,Y=y+b;if(X<0||Y<0||X>=W||Y>=H)continue;var k=Math.max(Math.abs(a),Math.abs(b))*4/rr/2;use[Y*W+X]+=k<.5?2.5:k<=1?1.6:.5;}});
  return simp(p);}
 function simp(p){var o=[p[0]];for(var k=1;k<p.length;k++){var q=p[k],a=o[o.length-1];if(Math.abs(q.x-a.x)<.01&&Math.abs(q.y-a.y)<.01)continue;
   if(o.length>1){var z=o[o.length-2];if((Math.abs(z.x-a.x)<.01&&Math.abs(a.x-q.x)<.01)||(Math.abs(z.y-a.y)<.01&&Math.abs(a.y-q.y)<.01)){o[o.length-1]=q;continue;}}o.push(q);}return o;}
 /* two boxes stacked with a clear gap get a straight route */
 function straight(A,B,obs){var v=A.b<=B.t||B.b<=A.t,hz=A.r<=B.l||B.r<=A.l,others=obs.filter(function(o){return o!==A&&o!==B;});
  function lane(lo,hi,mk){var mid=(lo+hi)/2,best=null;for(var t=lo+8;t<=hi-8;t+=1){if(others.some(function(o){return hit(o,mk(t),0);}))continue;if(best===null||Math.abs(t-mid)<Math.abs(best-mid))best=t;}return best;}
  if(v&&!hz){var l=Math.max(A.l,B.l),r=Math.min(A.r,B.r),y1=A.b<=B.t?A.b:A.t,y2=A.b<=B.t?B.t:B.b;if(r-l<20||Math.abs(y2-y1)>120)return null;
   var x=lane(l,r,function(t){return {l:t-4,r:t+4,t:Math.min(y1,y2)+1,b:Math.max(y1,y2)-1};});return x===null?null:[{x:x,y:y1},{x:x,y:y2}];}
  if(hz&&!v){var t0=Math.max(A.t,B.t),b0=Math.min(A.b,B.b),x1=A.r<=B.l?A.r:A.l,x2=A.r<=B.l?B.l:B.r;if(b0-t0<20||Math.abs(x2-x1)>140)return null;
   var y=lane(t0,b0,function(t){return {t:t-4,b:t+4,l:Math.min(x1,x2)+1,r:Math.max(x1,x2)-1};});return y===null?null:[{x:x1,y:y},{x:x2,y:y}];}return null;}
 function rd(p){var f=function(n){return Math.round(n*10)/10;},s="M"+f(p[0].x)+" "+f(p[0].y);
  for(var k=1;k<p.length-1;k++){var a=p[k-1],b=p[k],n=p[k+1],l1=Math.abs(b.x-a.x)+Math.abs(b.y-a.y),l2=Math.abs(n.x-b.x)+Math.abs(n.y-b.y),q=Math.min(10,l1/2,l2/2);
   var ux=Math.sign(b.x-a.x),uy=Math.sign(b.y-a.y),vx=Math.sign(n.x-b.x),vy=Math.sign(n.y-b.y);
   s+="L"+f(b.x-ux*q)+" "+f(b.y-uy*q)+"Q"+f(b.x)+" "+f(b.y)+" "+f(b.x+vx*q)+" "+f(b.y+vy*q);}
  var e=p[p.length-1];return s+"L"+f(e.x)+" "+f(e.y);}
 /* a spot on the route where a badge of w x h fits clear of every box and badge */
 function spot(p,w,h,obs,taken,fromEnd){
  if(fromEnd){var e=p[p.length-1],f=p[p.length-2],hz0=Math.abs(e.y-f.y)<.01,L0=Math.abs(e.x-f.x)+Math.abs(e.y-f.y);
   for(var u=(hz0?w:h)/2+4;u<=L0-(hz0?w:h)/2+2&&u<=60;u+=2){var x0=e.x-Math.sign(e.x-f.x)*u,y0=e.y-Math.sign(e.y-f.y)*u,r0={l:x0-w/2,r:x0+w/2,t:y0-h/2,b:y0+h/2};
    if(obs.some(function(o){return hit(o,r0,3);})||taken.some(function(o){return hit(o,r0,4);}))continue;taken.push(r0);return {x:x0,y:y0};}}var segs=[],tot=0;for(var k=0;k<p.length-1;k++){var a=p[k],b=p[k+1],L=Math.abs(b.x-a.x)+Math.abs(b.y-a.y);segs.push({a:a,b:b,L:L,s:tot});tot+=L;}
  var best=null,bv=1e9;segs.forEach(function(sg,k){var hz=Math.abs(sg.a.y-sg.b.y)<.01,need=(hz?w:h)/2+3;if(sg.L<2*need)return;
   for(var t=need;t<=sg.L-need;t+=3){var x=sg.a.x+Math.sign(sg.b.x-sg.a.x)*t,y=sg.a.y+Math.sign(sg.b.y-sg.a.y)*t,r={l:x-w/2,r:x+w/2,t:y-h/2,b:y+h/2};
    if(obs.some(function(o){return hit(o,r,3);})||taken.some(function(o){return hit(o,r,4);}))continue;
    var at=sg.s+t,v=fromEnd?(tot-at)+(hz?0:30):Math.abs(at-tot/2);if(v<bv){bv=v;best={x:x,y:y};}}});
  if(!best){var m=tot/2;segs.some(function(sg){if(sg.s+sg.L>=m){var t=m-sg.s;best={x:sg.a.x+Math.sign(sg.b.x-sg.a.x)*t,y:sg.a.y+Math.sign(sg.b.y-sg.a.y)*t};return true;}});}
  if(best)taken.push({l:best.x-w/2,r:best.x+w/2,t:best.y-h/2,b:best.y+h/2});return best;}
 function idw(id){return q('[data-idw="'+id+'"]');}
 function layout(g,defs){var obs=obstacles(g),sc2=getComputedStyle(sc),pl=parseFloat(sc2.paddingLeft),pr=parseFloat(sc2.paddingRight),pb=parseFloat(sc2.paddingBottom);
  var pills={};defs.forEach(function(L){if(L[2]==="cons"){var w=idw(L[1]),had=w.classList.contains("nr");w.classList.remove("nr");pills[L[1]]={w:w.offsetWidth,h:w.offsetHeight};if(had)w.classList.add("nr");}});
  var key=JSON.stringify([g.width|0,g.height|0,defs,obs.map(function(o){return [o.l|0,o.t|0,o.r|0,o.b|0];}),pills]);
  if(RT.key===key)return RT.geo;
  var c=g.width<560?2:4,x0=-Math.max(8,Math.min(26,pl-6)),y0=-10,G={c:c,x0:x0,y0:y0,W:Math.ceil((g.width-x0+Math.max(8,Math.min(26,pr-6)))/c),H:Math.ceil((g.height-y0+Math.max(6,Math.min(26,pb-4)))/c)},use=new Float32Array(G.W*G.H);
  var geo={r:[],pills:{}},taken=[],stacked=box(q('[data-zone="cons"]'),g).t>=box(q('[data-zone="cloud"]'),g).b-1;
  defs.forEach(function(L){var a=tile(L[0]==="laptop"?L[3]:L[0]),b=tile(L[1]);if(!a||!b||!a.offsetParent||!b.offsetParent)return;
   var A=box(a,g),B=box(b,g);obs.forEach(function(o){if(Math.abs(o.l-A.l)<.5&&Math.abs(o.t-A.t)<.5&&Math.abs(o.r-A.r)<.5)A=o;if(Math.abs(o.l-B.l)<.5&&Math.abs(o.t-B.t)<.5&&Math.abs(o.r-B.r)<.5)B=o;});
   var p;
   if(L[2]==="stub"){var dx=(B.l+B.r-A.l-A.r)/2,dy=(B.t+B.b-A.t-A.b)/2,e;
    if(Math.abs(dx)>Math.abs(dy)){e=dx<0?{x:A.l,y:(A.t+A.b)/2}:{x:A.r,y:(A.t+A.b)/2};p=[e,{x:e.x+(dx<0?-30:30),y:e.y}];}
    else{e=dy<0?{x:(A.l+A.r)/2,y:A.t}:{x:(A.l+A.r)/2,y:A.b};p=[e,{x:e.x,y:e.y+(dy<0?-24:24)}];}
    geo.r.push({L:L,p:p,mk:p[1]});return;}
   p=straight(A,B,obs);
   if(p){var i0=Math.floor((p[0].x-G.x0)/c),j0=Math.floor((p[0].y-G.y0)/c),i1=Math.floor((p[1].x-G.x0)/c),j1=Math.floor((p[1].y-G.y0)/c);
    for(var y=Math.min(j0,j1);y<=Math.max(j0,j1);y++)for(var x=Math.min(i0,i1);x<=Math.max(i0,i1);x++)if(x>=0&&y>=0&&x<G.W&&y<G.H)use[y*G.W+x]+=2.5;}
   else{var sa=null,sb=null;if(L[2]==="cons"){sa=stacked?1:0;sb=(sa+2)%4;}
    p=search(A,B,obs,G,use,sa,sb,L[2]==="cons"?(stacked?32:Math.max(12,B.l-A.r-28)):0);}
   if(!p)return;geo.r.push({L:L,p:p});});
  var mobs=obs.slice();
  /* badges sit centred on the stroke: a CLI user's at the middle of the gap it crosses, a lock at the middle of the route's longest run */
  geo.r.forEach(function(r){if(r.L[2]!=="cons")return;var e=r.p[r.p.length-1],f=r.p[r.p.length-2],hz=Math.abs(e.y-f.y)<.01,A=box(tile("registry"),g),B=box(tile(r.L[1]),g),
    u=hz?Math.min(Math.abs(e.x-f.x),Math.abs(B.l-A.r))/2:Math.min(Math.abs(e.y-f.y),32)/2;
   geo.pills[r.L[1]]={x:e.x-Math.sign(e.x-f.x)*u,y:e.y-Math.sign(e.y-f.y)*u};});
  geo.r.forEach(function(r){if(r.L[2]!=="net")return;var bi=0,bl=-1;for(var k=0;k<r.p.length-1;k++){var l=Math.abs(r.p[k+1].x-r.p[k].x)+Math.abs(r.p[k+1].y-r.p[k].y);if(l>bl){bl=l;bi=k;}}
   var m={x:(r.p[bi].x+r.p[bi+1].x)/2,y:(r.p[bi].y+r.p[bi+1].y)/2},bx={l:m.x-10,r:m.x+10,t:m.y-10,b:m.y+10};
   r.mk=mobs.some(function(o){return hit(o,bx,2);})?spot(r.p,20,20,mobs,taken,false):m;});
  RT.key=key;RT.geo=geo;return geo;}
 var LK='<path d="M-3.5 -.5h7v4.5h-7z"/>',LKC='<path d="M-2 -.5v-1.8a2 2 0 0 1 4 0v1.8"/>',LKO='<path d="M-2 -.5v-1.8a2 2 0 0 1 3.9 -.6"/>';
 /* Routes are kept as elements. A new route draws from source to target; its badge fades in after.
    During a move, routes touching what moves retract first, and nothing new appears until every glide has stopped. */
 var DR={},RH=0,DRAWP=Promise.resolve(),FIRST=true,mks=document.createElementNS(NS,"g");mks.setAttribute("class","mks");svg.insertBefore(mks,fx);
 svg.insertAdjacentHTML("afterbegin",'<defs><marker id="hx-ar" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="4.4" markerHeight="4.4" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z"/></marker></defs>');
 function rkey(L){return L[0]+"|"+L[1]+"|"+L[2];}
 function still(){return quick()||RM.matches;}
 function fade(el,a,b,ms){if(!el)return Promise.resolve();if(still()||!el.animate){el.style.opacity=b;return Promise.resolve();}
  var an=track(el.animate([{opacity:a},{opacity:b}],{duration:ms,fill:"forwards"}));
  return an.finished.then(null,function(){}).then(function(){el.style.opacity=b;try{an.cancel();}catch(e){}});}
 function drop(k){var e=DR[k];if(!e)return;delete DR[k];[e.el,e.mk].forEach(function(x){if(x&&x.parentNode)x.parentNode.removeChild(x);});}
 /* retract: the badge fades, then the line pulls back into its source */
 function retract(keys){if(!keys.length)return Promise.resolve();
  return Promise.all(keys.map(function(k){var e=DR[k];if(!e)return null;e.ready=false;pillsPaint();
   if(still()||!e.el.animate){drop(k);return null;}
   return fade(e.mk,1,0,160).then(function(){var len=e.el.getTotalLength();e.el.style.strokeDasharray=len+" "+len;
    var an=track(e.el.animate([{strokeDashoffset:0},{strokeDashoffset:len}],{duration:300,easing:"cubic-bezier(.4,0,1,1)",fill:"forwards"}));
    return an.finished.then(null,function(){});}).then(function(){drop(k);});})).then(function(){pillsPaint();});}
 function pillsPaint(){var geo=RT.geo||{pills:{}},g=flow.getBoundingClientRect();
  S.cons.forEach(function(c){var w=idw(c.id),e=DR["registry|"+c.id+"|cons"],p=geo.pills[c.id],t=tile(c.id);if(!w)return;
   if(!e||!p||!t){w.classList.add("nr");return;}w.classList.remove("nr");w.classList.toggle("pre",!e.ready);var tb=box(t,g);
   w.style.left=(p.x-w.offsetWidth/2-tb.l-t.clientLeft).toFixed(1)+"px";w.style.top=(p.y-w.offsetHeight/2-tb.t-t.clientTop).toFixed(1)+"px";});}
 function mkEl(r,ok){var m=document.createElementNS(NS,"g");m.setAttribute("class","mk "+(ok?"net":"stub"));m.setAttribute("transform","translate("+r.mk.x.toFixed(1)+" "+r.mk.y.toFixed(1)+")");
  m.innerHTML='<circle r="9"/>'+(ok?LK+(S.tls?LKC:LKO):'<path d="M-3.6 3.6L3.6 -3.6"/>');mks.appendChild(m);return m;}
 function draw(){if(flow.querySelector(".gliding"))return;
  var g=flow.getBoundingClientRect();svg.setAttribute("viewBox","0 0 "+g.width+" "+g.height);links=[];
  var defs=linkDefs(),geo=layout(g,defs),seen={},born=[],anim=!FIRST&&!still();
  geo.r.forEach(function(r){var L=r.L,k=rkey(L),d=rd(r.p),e=DR[k],ok=L[2]!=="stub",a=ACT[L[0]+"|"+L[1]],resized=false;
   if(e&&e.d!==d){if(RH>0)return;drop(k);e=null;resized=true;}
   if(!e){if(RH>0)return;var el=document.createElementNS(NS,"path");el.setAttribute("d",d);el.setAttribute("data-k",k);base.appendChild(el);
    e=DR[k]={el:el,d:d,mk:null,ready:true,L:L};if(anim&&!resized){e.ready=false;born.push(e);}}
   seen[k]=1;var on=a&&a.n>0,cls=L[2]==="stub"?"stub":L[2]==="local"?"local":"net";e.el.setAttribute("class",cls+(on?" act":""));
   e.el.removeAttribute("marker-start");e.el.removeAttribute("marker-end");if(on)e.el.setAttribute(a.rev?"marker-start":"marker-end","url(#hx-ar)");
   var sig=r.mk?[r.mk.x|0,r.mk.y|0,ok,S.tls].join():"";if(sig!==e.sig){if(e.mk&&e.mk.parentNode)e.mk.parentNode.removeChild(e.mk);e.mk=r.mk?mkEl(r,ok):null;e.sig=sig;if(e.mk&&!e.ready)e.mk.style.opacity=0;}
   links.push({a:L[0],b:L[1],d:d,el:e.el});});
  Object.keys(DR).forEach(function(k){if(!seen[k]&&RH===0)drop(k);});
  FIRST=false;pillsPaint();
  if(born.length){born.forEach(function(e){var len=e.el.getTotalLength();e.el.style.strokeDasharray=len+" "+len;e.el.style.strokeDashoffset=len;});
   DRAWP=Promise.all(born.map(function(e,j){return wait(j*70).then(function(){var len=e.el.getTotalLength();
     var an=track(e.el.animate([{strokeDashoffset:len},{strokeDashoffset:0}],{duration:460,easing:"cubic-bezier(.3,0,.2,1)",fill:"forwards"}));
     return an.finished.then(null,function(){}).then(function(){e.el.style.strokeDasharray="";e.el.style.strokeDashoffset="";try{an.cancel();}catch(x){}});})
    .then(function(){e.ready=true;pillsPaint();return fade(e.mk,0,1,200);}).then(function(){if(e.mk)e.mk.style.opacity="";});}));}}
 function link(a,b){var i,x;for(i=0;i<links.length;i++){x=links[i];if(x.a===a&&x.b===b)return {l:x,rev:false,el:x.el};if(x.a===b&&x.b===a)return {l:x,rev:true,el:x.el};}
  if(a==="cli"||a==="agent")a="laptop";if(b==="cli"||b==="agent")b="laptop";
  for(i=0;i<links.length;i++){x=links[i];if((x.a===a&&x.b===b)||(x.a===b&&x.b===a))return {l:x,rev:x.a!==a,el:x.el};}}
 /* a parcel with a light trail travels along a path */
 function travel(a,b,label,kind,dur){var f=link(a,b);if(!f||!f.el)return wait(140);if(quick())return Promise.resolve();
  dur=(dur||520)*SP;var L=f.el.getTotalLength(),seg=Math.min(110,L*.5),ak=f.l.a+"|"+f.l.b,ac=ACT[ak]||(ACT[ak]={n:0});ac.n++;ac.rev=f.rev;draw();
  var tr=document.createElementNS(NS,"path");tr.setAttribute("d",f.l.d);tr.setAttribute("class","trail");tr.style.strokeDasharray=seg+" "+(L+seg);fx.appendChild(tr);
  var ease="cubic-bezier(.6,0,.25,1)";
  var a1=track(tr.animate([{strokeDashoffset:f.rev?-L:seg},{strokeDashoffset:f.rev?seg:-L}],{duration:dur*1.12,easing:ease,fill:"both"}));
  var p=document.createElement("div");p.className="parcel"+(kind==="art"?" art":kind==="req"?" req":"");p.setAttribute("aria-hidden","true");p.textContent=label;
  p.style.offsetPath='path("'+f.l.d+'")';flow.appendChild(p);
  var s0=f.rev?"100%":"0%",s1=f.rev?"0%":"100%";
  var a2=track(p.animate([{offsetDistance:s0,transform:"scale(.4)",opacity:0},{offset:.14,transform:"scale(1)",opacity:1},{offset:.88,opacity:1,transform:"scale(1)"},{offsetDistance:s1,transform:"scale(.7)",opacity:0}],{duration:dur,easing:ease,fill:"both"}));
  function done(){if(p.parentNode)p.parentNode.removeChild(p);if(tr.parentNode)tr.parentNode.removeChild(tr);ac.n=Math.max(0,ac.n-1);draw();}
  return Promise.all([a1.finished,a2.finished]).then(done,done);}
 function sweep(a,b){var f=link(a,b);if(!f||!f.el||quick())return Promise.resolve();var L=f.el.getTotalLength(),seg=L*.6;
  var tr=document.createElementNS(NS,"path");tr.setAttribute("d",f.l.d);tr.setAttribute("class","trail");tr.style.strokeDasharray=seg+" "+(L+seg);fx.appendChild(tr);
  var an=track(tr.animate([{strokeDashoffset:f.rev?-L:seg},{strokeDashoffset:f.rev?seg:-L}],{duration:520*SP,easing:"cubic-bezier(.3,0,.2,1)",fill:"both"}));
  function d(){if(tr.parentNode)tr.parentNode.removeChild(tr);}return an.finished.then(d,d);}
 function accent(){return getComputedStyle(sc).getPropertyValue("--accent-fill").trim()||"#d9531a";}
 function land(el){if(quick()||!el.animate)return;var c=accent();
  track(el.animate([{transform:"translateY(-7px) scale(1.1)",boxShadow:"0 0 0 0 "+c},{offset:.5,transform:"translateY(1px) scale(.97)"},{transform:"none",boxShadow:"0 0 0 8px transparent"}],{duration:460,easing:"cubic-bezier(.3,1.3,.5,1)"}));}
 function ping(el){if(quick()||!el.animate)return;var c=accent();track(el.animate([{boxShadow:"0 0 0 0 "+c},{boxShadow:"0 0 0 9px transparent"}],{duration:520,easing:"ease-out"}));}

 /* ---------- paint ---------- */
 function ready(c){return S.cur&&PULL.every(function(n){return S.reg.has(S.cur.a[n]);})&&reach("registry",c.id)&&allowed(c);}
 function paint(){if(!S.cur)return;var ns=0,na=0,sn=0;
  SRCS.forEach(function(n){var e=sh("s:"+n),d=S.cur.s[n],on=S.reg.has(d);if(on)ns++;e.classList.toggle("on",on);e.lastChild.textContent=on?d.slice(0,6):"";e.title=n+" source "+d;});
  ARTS.forEach(function(n){var e=sh("a:"+n),d=S.cur.a[n],on=S.reg.has(d);if(on)na++;if(S.store.has(d))sn++;e.classList.toggle("on",on);e.lastChild.textContent=on?d.slice(0,6):"";e.title=n+" "+d;});
  q('[data-count="src"]').textContent=ns+"/3";q('[data-count="art"]').textContent=na+"/5";
  if(!S.busy){m("cli").textContent="you · store "+sn+"/5";
   m("agent").textContent=S.note.agent||"checks Vorpal.lock, packs, pushes";st("agent",S.note.agent?"err":null);
   S.workers.forEach(function(w){var n=S.note[w];if(!n&&S.same&&S.same===w){m(w).textContent="same build, same digest, different hardware";st(w,"live");return;}
    m(w).textContent=n||((S.where[w]==="laptop"?"on your laptop":"in the cloud")+(S.buildOn===w?" · aarch64-darwin":" · idle"));st(w,n?"err":null);});
   ["registry","w1"].forEach(function(id){["laptop","cloud"].forEach(function(z){q('[data-ghost="'+id+"-"+z+'"]').hidden=S.where[id]===z;});});storyPaint();}
  m("lock").textContent="pins uv "+S.lock+(S.uv!==S.lock?" · uv "+S.uv+" requested":"");
  m("registry").textContent=S.where.registry==="laptop"?"local socket · this machine only":"shared · S3 or local disk";
  S.cons.forEach(function(c){var e=m(c.id),has=c.has&&c.has===S.cur.a.greeter;if(c.busy)return;
   if(has){e.innerHTML='has greeter <span class="dg">'+c.has.slice(0,6)+"</span>";st(c.id,"live");}
   else if(!c.err&&!reach("registry",c.id)){e.textContent="can't reach your laptop";st(c.id,"off");}
   else if(c.err||!allowed(c)){e.innerHTML='<svg class="mi" aria-hidden="true"><use href="#hxi-ban"/></svg>'+esc(c.err||"no access");st(c.id,"err");}
   else{e.textContent=ready(c)?"CLI · ready to pull":!reach("registry",c.id)?"can't reach registry":"uses the CLI";st(c.id,null);}
   var w=q('[data-idw="'+c.id+'"]'),tog=S.oidc&&c.kind==="human",key=tog?"b":"s";
   var txt=!S.oidc?"open":c.kind==="service"?"service":c.access==="read"?"read":"no access";
   if(w.dataset.k!==key){w.dataset.k=key;w.innerHTML=tog?'<button type="button" class="badge"><svg aria-hidden="true"><use href="#hxi-lk"/></svg><span></span></button>':'<span class="badge"><svg aria-hidden="true"><use href="#hxi-lk"/></svg><span></span></span>';
    if(tog)w.firstChild.addEventListener("click",function(){toggleAccess(c.id);});}
   w.firstChild.lastChild.textContent=txt;w.firstChild.classList.toggle("no",txt==="no access");w.firstChild.firstChild.firstChild.setAttribute("href",S.tls?"#hxi-lk":"#hxi-ulk");
   if(tog)w.firstChild.setAttribute("aria-label",c.name+" access: "+txt+". Change");else w.firstChild.title=!S.oidc?"OIDC sign-in is off":"trusted service client ID";});
  if(!S.busy){btn.textContent=S.once&&ARTS.every(function(n){return S.store.has(S.cur.a[n]);})?"Build again":"Build";btn.removeAttribute("data-skip");}
  draw();if(P.open)place();}

 /* ---------- popover ---------- */
 var pop=document.createElement("div");pop.className="pop";pop.hidden=true;pop.setAttribute("role","dialog");pop.setAttribute("aria-labelledby","pop-h");
 pop.innerHTML='<div class="ph"><h3 id="pop-h"></h3><button type="button" class="x" aria-label="Close">×</button></div><div class="pb"></div>';
 var P={open:null,trig:null,anchor:null};
 function place(){var f=flow.getBoundingClientRect(),a=P.anchor.getBoundingClientRect(),w=pop.offsetWidth,h=pop.offsetHeight;
  var left=Math.max(0,Math.min(a.left-f.left,f.width-w)),top=a.bottom-f.top+6;
  if(a.bottom+h+8>innerHeight&&a.top-h-6>0)top=a.top-f.top-h-6;
  pop.style.left=left+"px";pop.style.top=top+"px";}
 function close(back){if(!P.open)return;var t=P.trig;pop.hidden=true;if(t&&t.hasAttribute("aria-expanded"))t.setAttribute("aria-expanded","false");P.open=null;if(back&&t&&document.contains(t))t.focus();}
 function open(kind,id,title,body,focusSel){close(false);var anc=tile(id)||T[id],trig=T[id]||null;
  P.open=kind+":"+id;P.trig=trig;P.anchor=anc;P.id=id;pop.querySelector("#pop-h").textContent=title;pop.querySelector(".pb").innerHTML=body;
  anc.parentNode.insertBefore(pop,anc.nextSibling);pop.hidden=false;if(trig&&trig.hasAttribute("aria-expanded"))trig.setAttribute("aria-expanded","true");place();
  var f=pop.querySelector(focusSel||"input,select,.pb button");if(f)f.focus();wire();}
 pop.querySelector(".x").addEventListener("click",function(){close(true);});
 pop.addEventListener("keydown",function(e){if(e.key==="Escape"){e.preventDefault();close(true);}});
 document.addEventListener("pointerdown",function(e){if(P.open&&!pop.contains(e.target)&&!(P.trig&&P.trig.contains(e.target)))close(false);});
 document.addEventListener("focusin",function(e){if(P.open&&!pop.contains(e.target)&&e.target!==P.trig)close(false);});
 function runsOn(id){var z=S.where[id];return '<span class="pl" id="ro">Runs on</span><div class="row" role="group" aria-labelledby="ro"><button type="button" class="btn" data-to="laptop" aria-pressed="'+(z==="laptop")+'">Laptop</button><button type="button" class="btn" data-to="cloud" aria-pressed="'+(z==="cloud")+'">Cloud</button></div>';}
 function openItem(id){stopDemo();if(P.open&&P.id===id&&P.open.indexOf("item:")===0){close(true);return;}
  if(id==="main")open("item","main","src/main.py",'<label for="pin">Edit the line</label><input id="pin" spellcheck="false" autocomplete="off" value="'+esc(S.main)+'"><p>The digest updates as you type.</p>');
  else if(id==="lock")open("item","lock","Vorpal.lock",'<label for="puv">uv version</label><select id="puv"><option value="0.13.8"'+(S.uv==="0.13.8"?" selected":"")+'>0.13.8 (pinned)</option><option value="0.13.9"'+(S.uv==="0.13.9"?" selected":"")+'>0.13.9 (example bump)</option></select><p>The agent checks this against the lock.</p>');
  else if(id==="registry")open("item","registry","Registry",runsOn("registry")+'<span class="pl" id="sv">Registry and worker</span><div class="row" role="group" aria-labelledby="sv"><button type="button" class="btn" data-tg="tls" aria-pressed="'+S.tls+'">TLS</button><button type="button" class="btn" data-tg="oidc" aria-pressed="'+S.oidc+'">OIDC sign-in</button></div>','[data-to][aria-pressed="false"]');
  else open("item",id,wname(id),runsOn(id)+(S.workers.length>1?'<div class="row" style="margin-top:8px"><button type="button" class="btn" data-act="usew" aria-pressed="'+(S.buildOn===id)+'">Use for builds</button></div>':""),'[data-to][aria-pressed="false"]');}
 function wire(){var b=pop.querySelector(".pb");
  var pin=b.querySelector("#pin");if(pin)pin.addEventListener("input",function(){S.main=pin.value;clearTimeout(wire.t);wire.t=setTimeout(recompute,220);});
  var puv=b.querySelector("#puv");if(puv)puv.addEventListener("change",function(){S.uv=puv.value;S.note.agent=null;recompute();});
  $$("[data-to]",b).forEach(function(x){x.addEventListener("click",function(){var id=P.id;moveTo(id,x.dataset.to);close(false);openItem(id);var f=pop.querySelector('[data-to="'+x.dataset.to+'"]');if(f)f.focus();});});
  $$("[data-tg]",b).forEach(function(x){x.addEventListener("click",function(){var k=x.dataset.tg;S[k]=!S[k];x.setAttribute("aria-pressed",String(S[k]));S.cons.forEach(function(c){c.err=null;});paint();
   say(k==="tls"?(S.tls?"TLS on: paths between machines are encrypted.":"TLS off: paths between machines are plaintext."):(S.oidc?"OIDC sign-in on: the registry and worker require a valid token; pulls need read on the namespace.":"OIDC sign-in off: anyone who can reach the registry can pull."));});});
  var u=b.querySelector('[data-act="usew"]');if(u)u.addEventListener("click",function(){S.buildOn=P.id;u.setAttribute("aria-pressed","true");paint();say("The CLI now builds with "+wname(P.id)+" (--worker).");});
  var un=b.querySelector('[data-act="unlock"]');if(un)un.addEventListener("click",function(){close(false);btn.focus();build(true);});
  var gr=b.querySelector('[data-act="grant"]');if(gr)gr.addEventListener("click",function(){var c=cons(P.id);c.access="read";c.err=null;close(false);paint();T[c.id].focus();say(c.name+" now has read access to library.");});
  var mr=b.querySelector('[data-act="mvreg"]');if(mr)mr.addEventListener("click",function(){var back=P.trig;close(false);moveTo("registry","cloud");if(back&&back.focus)back.focus();});}

 /* ---------- moves ---------- */
 /* ---------- moves: retract the routes that change, glide, then draw the new routes ---------- */
 function put(id,z){S.where[id]=z;var n=tile(id);
  if(id==="w1"||id==="registry"){var gh=q('[data-ghost="'+id+"-"+z+'"]');gh.parentNode.insertBefore(n,gh);}
  else if(z==="laptop")q('[data-slot="laptop"]').appendChild(n);else q("[data-workers]").appendChild(n);}
 function ghosts(){["registry","w1"].forEach(function(id){["laptop","cloud"].forEach(function(z){var e=q('[data-ghost="'+id+"-"+z+'"]');if(e)e.hidden=S.where[id]===z;});});}
 /* the routes as they will be after the moves: move, measure and move back in one task, so nothing is painted */
 function predict(list){var save=list.map(function(m){var n=tile(m[0]);return {id:m[0],w:S.where[m[0]],n:n,p:n.parentNode,nx:n.nextSibling};}),
   gh=$$("[data-ghost]",sc).map(function(x){return [x,x.hidden];});
  list.forEach(function(m){put(m[0],m[1]);});ghosts();
  var g=flow.getBoundingClientRect(),geo=layout(g,linkDefs()),out={};geo.r.forEach(function(r){out[rkey(r.L)]=rd(r.p);});
  save.reverse().forEach(function(s){S.where[s.id]=s.w;s.p.insertBefore(s.n,s.nx);});gh.forEach(function(x){x[0].hidden=x[1];});RT.key="";
  return out;}
 function glide(n,r0){var r1=n.getBoundingClientRect(),dx=r0.left-r1.left,dy=r0.top-r1.top;if(!(dx||dy)||!n.animate||still())return Promise.resolve();
  n.classList.add("gliding");var a=track(n.animate([{transform:"translate("+dx+"px,"+dy+"px)"},{transform:"none"}],{duration:1200,easing:"cubic-bezier(.65,0,.3,1)"}));
  return a.finished.then(null,function(){}).then(function(){n.classList.remove("gliding");});}
 function transit(list,o){o=o||{};list=list.filter(function(m){return S.where[m[0]]!==m[1];});if(S.busy||!list.length)return Promise.resolve();
  var fut=predict(list),gone=Object.keys(DR).filter(function(k){return fut[k]!==DR[k].d;}),r0={};
  list.forEach(function(m){r0[m[0]]=o.from&&list.length===1?o.from:tile(m[0]).getBoundingClientRect();});
  RH++;var fin=function(){RH=Math.max(0,RH-1);paint();return DRAWP;};
  return retract(gone).then(function(){
   list.forEach(function(m){put(m[0],m[1]);if(m[0]==="registry"&&!o.quiet)reset();});
   S.note={};S.same=null;S.cons.forEach(function(c){c.err=null;});paint();
   if(!o.quiet){var id=list[0][0],z=list[0][1];say(id==="registry"?(z==="laptop"?"Registry moved to your laptop. On a local socket, nothing else can reach it. It starts empty: press Build.":"Registry moved to the cloud. Everyone you allow can reach it. It starts empty: press Build."):wname(id)+" moved to "+(z==="laptop"?"your laptop.":"the cloud.")+(id==="w1"?" It builds the same digests there; artifacts already in the registry are reused.":""));}
   return Promise.all(list.map(function(m){return glide(tile(m[0]),r0[m[0]]);}));}).then(fin,function(e){fin();throw e;});}
 function moveTo(id,z,o){return transit([[id,z]],o);}

 function recompute(){return define({main:S.main,uv:S.uv,job:"greeter"}).then(function(d){var prev=S.cur;S.cur=d;
  scramble(m("main"),"sha256 ",d.s.greeter.slice(0,8),prev?360:0);paint();
  if(prev){var ch=ARTS.filter(function(n){return prev.a[n]!==d.a[n];}),cs=SRCS.filter(function(n){return prev.s[n]!==d.s[n];});
   say((cs.length?"New source digest: "+list(cs)+". ":"")+(ch.length?"New artifact digests: "+list(ch)+"; they are not in the registry yet.":"No digest changed.")+(S.uv!==S.lock?" uv is not pinned at this version.":""));}});}

 /* ---------- build: the showpiece ---------- */
 function build(unlock,demo){if(!S.cur)return;if(S.busy){skipAll();return;}close(false);var cur=S.cur;S.note={};
  if(S.uv!==S.lock&&!unlock){S.note.agent="stopped: uv changed";paint();
   open("note","agent","Agent stopped",'<p class="e">source \'uv\' changed - use \'--unlock\' to update</p><button type="button" class="btn ink" data-act="unlock">Build with --unlock</button>');
   say("The agent stopped the build: source uv changed, use --unlock to update. Focus is on Build with --unlock.");return;}
  var wk=S.buildOn;
  if(S.where.registry==="laptop"&&S.where[wk]==="cloud"){S.note[wk]="can't reach registry";paint();
   open("note",wk,wname(wk)+" can't reach the registry",'<p>The registry is on your laptop\'s local socket.</p><button type="button" class="btn ink" data-act="mvreg">Move registry to cloud</button>');
   say("Build stopped: the cloud worker can't reach a registry on your laptop. Focus is on Move registry to cloud.");return;}
  S.busy=true;S.skip=false;btn.textContent="Skip";btn.setAttribute("data-skip","");btn.setAttribute("aria-label","Skip animation");
  S.same=null;var place=S.where[wk];var relock=S.uv!==S.lock;if(relock)S.lock=S.uv;var pushed=[],built=[],pulled=[],tail=[],rippled=false;
  if(!demo)say("Build started. Press Skip to jump to the result.");
  function stc(){var k=0;ARTS.forEach(function(n){if(S.store.has(cur.a[n]))k++;});m("cli").textContent="you · store "+k+"/5";}
  function ripple(){rippled=true;S.cons.forEach(function(c,i){if(!ready(c))return;wait(i*110).then(function(){return sweep("registry",c.id);}).then(function(){ping(tile(c.id));paint();});});}
  S.workers.forEach(function(w){wstep(w,0);});st("agent","work");m("agent").textContent=relock?"Vorpal.lock updated to uv "+S.lock:"checking sources against Vorpal.lock";paint();
  return wait(300).then(function(){var miss=SRCS.filter(function(n){return !S.reg.has(cur.s[n]);});
   if(miss.length)m("agent").textContent="packing "+list(miss)+" (zstd)";
   return Promise.all(miss.map(function(n,i){return wait(i*130).then(function(){return travel("agent","registry",n,"src",420);}).then(function(){S.reg.add(cur.s[n]);pushed.push(n);paint();land(sh("s:"+n));});}));
  }).then(function(){st("agent",null);m("agent").textContent=pushed.length?"sources pushed":"sources already in registry";var c=Promise.resolve();
   var nb=0;ARTS.forEach(function(n,i){c=c.then(function(){var d=cur.a[n];if(S.store.has(d))return;
    if(S.reg.has(d)){m("cli").textContent="pulling "+n;return travel("registry","cli",n,"art",420).then(function(){S.store.add(d);pulled.push(n);stc();paint();});}
    var k=[1,.8,.7,.62,.62][Math.min(nb++,4)],wm=m(wk),bar=tile(wk).querySelector(".prog");st(wk,"work");wm.textContent="pulling sources";wstep(wk,1);wnow(wk,"<b>"+n+"</b>");
    return Promise.all([travel("cli",wk,"build "+n,"req",280*k),wait(50*k).then(function(){return travel("registry",wk,n,"src",280*k);})]).then(function(){
      wm.textContent="running steps in sandbox";wstep(wk,2);if(bar&&!quick())track(bar.animate([{transform:"scaleX(0)"},{transform:"scaleX(1)",offset:.85},{transform:"scaleX(1)",opacity:0}],{duration:300*k,easing:"ease-in-out"}));
      scramble(q('[data-now="'+wk+'"]'),"<b>"+n+"</b> ",d.slice(0,6),280*k);return Promise.all([wait(300*k),travel(wk,"cli","logs","req",300*k)]);})
     .then(function(){wstep(wk,3);wm.textContent="pushing archive";return travel(wk,"registry",n,"art",300*k);})
     .then(function(){wstep(wk,4);S.reg.add(d);built.push(n);paint();st(wk,"work");land(sh("a:"+n));if(!rippled&&n==="greeter")ripple();
      tail.push(travel("registry","cli",n,"art",360).then(function(){S.store.add(d);stc();}));});});});
   return c;
  }).then(function(){return Promise.all(tail);}).then(function(){
   S.busy=false;S.skip=false;S.once=true;btn.removeAttribute("aria-label");
   if(built.length){var gd=cur.a["greeter-test"];if(S.built&&S.built.place!==place&&S.built.d===gd)S.same=wk;S.built={place:place,d:gd};}
   if(!rippled)ripple();paint();
   var t=(pushed.length?"Pushed "+pushed.length+" source"+(pushed.length>1?"s":"")+". ":"")+(built.length?"Built "+list(built)+". ":"")+(pulled.length?"Pulled "+list(pulled)+". ":"")+
    (pushed.length+built.length+pulled.length?"":"Nothing to do: every digest is already in your store. ");
   if(!demo)say("Done. "+t+(S.same?"Same digests as the build on the other machine. ":"")+(built.length?"Click a CLI user to pull.":""));});}

 function pull(id,demo){var c=cons(id);if(S.busy||!c||!S.cur||c.busy)return;close(false);var cur=S.cur;c.err=null;
  if(S.where.registry==="laptop"){c.err="can't reach registry";paint();
   open("note",id,c.name+" can't reach the registry",'<p>The registry is on your laptop\'s local socket.</p><button type="button" class="btn ink" data-act="mvreg">Move registry to cloud</button>');
   say(c.name+" can't reach the registry: it is on your laptop's local socket.");return;}
  if(!allowed(c)){c.err="denied · no read";paint();
   open("note",id,c.name+": token valid",'<p class="e">insufficient permissions: no read access to namespace: library</p><button type="button" class="btn ink" data-act="grant">Grant read</button>');
   say(c.name+": token valid, but insufficient permissions: no read access to namespace library.");return;}
  var miss=PULL.filter(function(n){return !S.reg.has(cur.a[n]);});
  if(miss.length){m(id).textContent="nothing to pull yet · Build first";st(id,null);say(c.name+": "+list(miss)+" not in the registry yet. Press Build first.");return;}
  var chk=!S.oidc?"no sign-in (OIDC off)":c.kind==="service"?"token valid · trusted service":"token valid · read on library";
  c.busy=true;st(id,"work");m(id).textContent="checking token…";
  return wait(260).then(function(){m(id).textContent=chk;return Promise.all(PULL.map(function(n,i){return wait(i*120).then(function(){return travel("registry",id,n,"art",480);});}));})
   .then(function(){c.busy=false;c.has=cur.a.greeter;st(id,"live");scramble(m(id),"has greeter ",c.has.slice(0,6),320);ping(tile(id));
    setTimeout(paint,quick()?0:360);if(!demo)say(c.name+": "+chk+". Pulled cpython, uv and greeter "+c.has.slice(0,8)+" from the registry. Built nothing.");});}

 function toggleAccess(id){stopDemo();var c=cons(id);c.access=c.access==="read"?"none":"read";c.err=null;paint();say(c.name+" access: "+(c.access==="read"?"read":"no access")+".");}

 /* ---------- events ---------- */
 ["main","lock","registry","w1"].forEach(function(k){T[k].addEventListener("click",function(){if(tile(k).__drag)return;openItem(k);});});
 btn.addEventListener("click",function(){if(D.on){var b=S.busy;stopDemo().then(function(){if(!b)build(false);});}else build(false);});
 q('[data-add="worker"]').addEventListener("click",function(){stopDemo();if(S.workers.length>1)return;
  var w=document.createElement("button");w.type="button";w.className="tile wk big mv";w.dataset.id="w2";w.dataset.move="w2";w.setAttribute("aria-haspopup","dialog");w.setAttribute("aria-expanded","false");
  w.innerHTML='<svg class="ic" aria-hidden="true"><use href="#hxi-gear"/></svg><span class="t">Worker 2</span><span class="m" data-m="w2"></span><span class="now"><span>Artifact</span><span data-now="w2">none yet</span></span><span class="steps"><span class="stp">pull sources</span><span class="stp">run steps in sandbox</span><span class="stp">push archive</span></span><i class="prog" aria-hidden="true"></i>';
  q("[data-workers]").appendChild(w);T.w2=w;tiles.w2=w;S.workers.push("w2");S.where.w2="cloud";bindDrag(w);w.addEventListener("click",function(){if(!w.__drag)openItem("w2");});
  this.setAttribute("aria-disabled","true");paint();ping(w);say("Worker 2 added in the cloud. Select it to build with it or move it.");});
 q('[data-add="cons"]').addEventListener("click",function(){stopDemo();if(S.added>=CONS_ADD.length)return;var c=addCons(CONS_ADD[S.added++]);
  if(S.added>=CONS_ADD.length)this.setAttribute("aria-disabled","true");paint();ping(tile(c.id));
  say("Added "+c.name+(c.kind==="service"?" as a trusted service.":" with no access to library."));});

 /* drag the registry or a worker between machines; keyboard: open it, then Runs on */
 function bindDrag(n){var sx,sy,drag=false;
  n.addEventListener("pointerdown",function(e){if(e.button!==0||S.busy)return;sx=e.clientX;sy=e.clientY;drag=false;n.__drag=false;n.__pid=e.pointerId;});
  n.addEventListener("pointermove",function(e){if(sx===undefined)return;var dx=e.clientX-sx,dy=e.clientY-sy;
   if(!drag&&Math.hypot(dx,dy)>6){stopDemo();drag=true;n.__drag=true;n.classList.add("dragging");close(false);try{n.setPointerCapture(n.__pid);}catch(x){}var mv=n.dataset.move;RH++;n.__rh=true;retract(Object.keys(DR).filter(function(k){var a=k.split("|");return a[0]===mv||a[1]===mv;}));}
   if(drag){n.style.transform="translate("+dx+"px,"+dy+"px)";$$(".zone",sc).forEach(function(z){var r=z.getBoundingClientRect();z.classList.toggle("drop",z.dataset.zone!=="cons"&&e.clientX>=r.left-12&&e.clientX<=r.right+12&&e.clientY>=r.top-12&&e.clientY<=r.bottom+12);});}});
  function end(){sx=undefined;if(n.__rh){n.__rh=false;RH=Math.max(0,RH-1);}n.classList.remove("dragging");n.style.transform="";var z=sc.querySelector(".zone.drop");$$(".zone",sc).forEach(function(x){x.classList.remove("drop");});return z;}
  n.addEventListener("pointerup",function(){if(sx===undefined)return;var r=n.getBoundingClientRect(),z=end();if(drag&&z&&S.where[n.dataset.move]!==z.dataset.zone)moveTo(n.dataset.move,z.dataset.zone,{from:r});else if(drag)paint();setTimeout(function(){n.__drag=false;},0);});
  n.addEventListener("pointercancel",function(){if(sx!==undefined){end();paint();}});}
 $$("[data-move]",sc).forEach(bindDrag);
 if("ResizeObserver" in window)new ResizeObserver(function(){draw();if(P.open)place();}).observe(flow);
 /* ---------- the demo: autoplay, loop, pause ---------- */
 var ctl=q('[data-act="demo"]');
 function ctlPaint(){var on=D.on&&!D.paused;ctl.lastChild.textContent=on?"Pause":"Play";ctl.setAttribute("aria-label",(on?"Pause":"Play")+" animation");ctl.querySelector("use").setAttribute("href",on?"#hxi-pause":"#hxi-play");}
 function idle(){return new Promise(function(r){(function chk(){if(!S.busy&&!S.cons.some(function(c){return c.busy;}))r();else setTimeout(chk,16);})();});}
 function stopDemo(){if(!D.on)return Promise.resolve();D.on=false;D.paused=false;sync();skipAll();ctlPaint();
  return idle().then(function(){S.skip=false;});}
 function reset(){S.reg.clear();S.store.clear();S.once=false;S.note={};S.cons.forEach(function(c){c.has=null;c.err=null;});S.workers.forEach(function(w){wstep(w,0);wnow(w,"none yet");});paint();}
 function demoPull(){var r=S.cons.filter(function(c){return ready(c);});if(!r.length)return;return pull(r[D.n++%r.length].id,true);}
 var CAP={1:"All local: everything runs on your laptop. Builds work, but no one else can reach them.",2:"Move the registry: everyone you allow re-uses your artifacts.",3:"Move the worker: share build machines too. Same build, same digest.",0:"Your own layout. Pick a stage to replay the story."};
 function stageOf(){var r=S.where.registry,w=S.where.w1;return r==="laptop"&&w==="laptop"?1:r==="cloud"&&w==="laptop"?2:r==="cloud"&&w==="cloud"?3:0;}
 function storyPaint(){var k=stageOf();$$("[data-stage]",sc).forEach(function(b){b.setAttribute("aria-pressed",String(+b.dataset.stage===k));});q("[data-cap]").textContent=CAP[k];}
 function toStage(k,glide){var t={1:["laptop","laptop"],2:["cloud","laptop"],3:["cloud","cloud"]}[k];close(false);S.buildOn="w1";reset();
  return transit([["registry",t[0]],["w1",t[1]]],{quiet:true}).then(function(){paint();});}
 function stage(k){if(!D.on)return;return toStage(k,true).then(function(){if(D.on&&!D.told[k]){D.told[k]=true;say(CAP[k]);}return D.on&&hold(400);});}
 function loop(){if(D.loop)return;D.loop=true;(function cycle(){if(!D.on){D.loop=false;return;}
  stage(1).then(function(){return D.on&&build(true,true);}).then(function(){return D.on&&hold(1600);})
   .then(function(){return stage(2);}).then(function(){return D.on&&build(true,true);}).then(function(){return D.on&&hold(300);}).then(function(){return D.on&&demoPull();}).then(function(){return D.on&&hold(1300);})
   .then(function(){return stage(3);}).then(function(){return D.on&&build(true,true);}).then(function(){return D.on&&hold(300);}).then(function(){return D.on&&demoPull();}).then(function(){return D.on&&hold(2200);})
   .then(cycle);})();}
 $$("[data-stage]",sc).forEach(function(b){b.addEventListener("click",function(){var k=+b.dataset.stage;
  function go(){toStage(k,true).then(function(){say("Stage "+k+". "+CAP[k]+" Press Build for a fresh build.");});b.focus();}if(D.on)stopDemo().then(go);else go();});});
 ctl.addEventListener("click",function(){
  if(D.on){D.paused=!D.paused;sync();ctlPaint();say(D.paused?"Animation paused.":"Animation playing.");return;}
  close(false);D.on=true;D.paused=false;ctlPaint();say("Animation playing. It repeats until you pause it.");loop();});
 if("IntersectionObserver" in window)new IntersectionObserver(function(es){D.vis=es[0].isIntersecting;D.susp=!D.vis||document.hidden;sync();},{threshold:.15}).observe(sc);
 document.addEventListener("visibilitychange",function(){D.susp=document.hidden||!D.vis;sync();});
 recompute().then(function(){paint();
  if(RM.matches){build(false,true);ctlPaint();}
  else{D.on=true;ctlPaint();hold(800).then(function(){if(D.on)loop();});}});
}
