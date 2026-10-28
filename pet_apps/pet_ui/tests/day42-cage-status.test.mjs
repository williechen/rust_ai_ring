import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert.ok(script, "test the actual Tauri bridge");
function boot() {
  const calls = [];
  const window = { location: { search: "" }, __TAURI__: {
    core: { invoke: async (name, args) => {
      calls.push([name, args]);
      if (args?.id === "broken") throw Error("SQLite read failed");
      return args?.id === "pet-b";
    } }, event: { listen: async () => () => {} }
  } };
  vm.runInNewContext(script, { window, URLSearchParams });
  return { window, calls };
}
test("each status query reaches the correct saved UUID and native result", async () => {
  const { window, calls } = boot();
  assert.equal(await window.petCageStatusById("pet-a"), false);
  assert.equal(await window.petCageStatusById("pet-b"), true);
  assert.deepEqual(calls, [
    ["pet_cage_status_by_id", { id: "pet-a" }],
    ["pet_cage_status_by_id", { id: "pet-b" }]
  ]);
});
test("native database failure is not reported as a successful cage state", async () => {
  const { window } = boot();
  await assert.rejects(window.petCageStatusById("broken"), /SQLite read failed/);
});