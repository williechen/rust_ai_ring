import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import test from "node:test";
import vm from "node:vm";
const script=readFileSync(new URL("../index.html",import.meta.url),"utf8").match(/<script>([\s\S]*?)<\/script>/)[1];
function boot(){const calls=[];const window={__TAURI__:{core:{invoke:async(name,args)=>{calls.push([name,structuredClone(args)]);if(args.id==="foreign")throw Error("拒絕其他主人");return args.profile??{"名字":"寵物","種類":"tortoise","喜好":[],"個性":[],"興趣":[]};}},event:{listen:async()=>()=>{}}}};vm.runInNewContext(script,{window,location:{search:""},URLSearchParams});return {window,calls};}
test("five-field settings use explicit selected id and preserve UTF8 profile",async()=>{const{window,calls}=boot();assert.equal(typeof window.loadPetSettings,"function");assert.equal(typeof window.savePetSettings,"function");const profile={"名字":"新名字","種類":"tortoise","喜好":["陽光"],"個性":["安靜"],"興趣":["散步"]};await window.loadPetSettings("A");assert.deepEqual(await window.savePetSettings("B",profile),profile);assert.deepEqual(calls,[["load_pet_settings",{id:"A"}],["save_pet_settings",{id:"B",profile}]]);});
test("settings native failures remain rejected promises",async()=>{const{window}=boot();assert.equal(typeof window.savePetSettings,"function");await assert.rejects(window.savePetSettings("foreign",{}),/拒絕其他主人/);});