import assert from "node:assert/strict";
process.env.MEMNEST_ALLOWED_MODEL_PROVIDERS = "approved-local";
process.env.MEMNEST_EXPOSE_SECRET_TOOLS = "1";
process.env.MEMNEST_AUTOCONTEXT_MODE = "balanced";
let calls = 0;
globalThis.fetch = async () => {
  calls++;
  return new Response(JSON.stringify({project:"alpha",results:[{id:"a",project:"alpha",score:1,document:"policy fixture",chunk_type:"Manual",doc_len:14,
    evidence:{quote:"policy fixture",assertion:"local_model_selected_not_truth_verified"}}]}));
};
const tools=new Map(), hooks=new Map();
(await import("../dist/index.mjs")).default({registerTool(t){tools.set(t.name,t);},registerCommand(){},on(n,f){hooks.set(n,f);}});
const blocked = {cwd:"/tmp/alpha",model:{provider:"unapproved-cloud"}};
const approved = {cwd:"/tmp/alpha",model:{provider:"approved-local"}};
for (const [name,args] of [["memory_search",{query:"What was the deployment decision?",project:"alpha"}], ["memory_get",{id:"a"}], ["secret_get",{key:"a"}], ["secret_list",{}]]) {
  const result=await tools.get(name).execute("x",args,null,null,blocked);
  assert.match(result.content[0].text,/retrieval blocked/);
}
assert.equal(calls,0);
const unknown=await tools.get("memory_get").execute("x",{id:"a"},null,null,{});
assert.match(unknown.content[0].text,/retrieval blocked/);
hooks.get("session_start")({},blocked);
assert.equal(await hooks.get("before_agent_start")({prompt:"Why did we choose this deployment design?"},blocked),undefined);
assert.equal(calls,0);
const result=await tools.get("memory_search").execute("x",{query:"policy fixture",project:"alpha",evidence_only:true},null,null,approved);
assert.match(result.content[0].text,/local_model_selected_not_truth_verified/);
assert.equal(calls,1);
console.log("PASS provider allowlist blocks search/get/credential reads and autocontext before network calls; unknown model fails closed; approved source quotes stay unverified");
