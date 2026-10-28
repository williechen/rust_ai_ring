import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert.ok(script, "must test the real Tauri bridge");

function boot() {
  const calls = [];
  const window = {
    location: { search: "" },
    __TAURI__: {
      core: { invoke: async (name, args) => {
        calls.push([name, structuredClone(args)]);
        if (args?.id === "broken") throw Error("native cage failed");
      } },
      event: { listen: async () => () => {} }
    }
  };
  vm.runInNewContext(script, { window, location: window.location, URLSearchParams });
  return { window, calls };
}

test("control can cage one explicit UUID without changing selection", async () => {
  const { window, calls } = boot();
  await window.cageSavedPetById("pet-b");
  assert.deepEqual(calls, [["cage_saved_pet", { id: "pet-b" }]]);
});

test("native cage error reaches control caller", async () => {
  const { window, calls } = boot();
  await assert.rejects(window.cageSavedPetById("broken"), /native cage failed/);
  assert.deepEqual(calls, [["cage_saved_pet", { id: "broken" }]]);
});