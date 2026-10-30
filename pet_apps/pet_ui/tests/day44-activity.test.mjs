import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import test from "node:test";
import vm from "node:vm";
const script=readFileSync(new URL("../index.html",import.meta.url),"utf8").match(/<script>([\s\S]*?)<\/script>/)[1];
function boot(){
  const calls=[];
  const window={__TAURI__:{core:{invoke:async(name,args)=>{calls.push([name,structuredClone(args)]);if(args.id==="broken")throw Error("SQLite unavailable");return {outside:true,roaming:!!args.walking,default_caged:true};}},event:{listen:async()=>()=>{}}}};
  vm.runInNewContext(script,{window,location:{search:""},URLSearchParams});return{window,calls};
}
test("walk/sit and defaults reach scoped native commands with booleans",async()=>{
 const{window,calls}=boot();await window.setPetWalk("A",true);await window.setPetWalk("A",false);await window.setPetDefaultCaged("B",false);await window.petActivityStatus("B");
 assert.deepEqual(calls,[["set_pet_walk",{id:"A",walking:true}],["set_pet_walk",{id:"A",walking:false}],["set_pet_default_caged",{id:"B",caged:false}],["pet_activity_status",{id:"B"}]]);
});
test("failed activity and default writes remain rejected promises",async()=>{
 const{window}=boot();await assert.rejects(window.setPetWalk("broken",true),/SQLite unavailable/);await assert.rejects(window.setPetDefaultCaged("broken",false),/SQLite unavailable/);
});