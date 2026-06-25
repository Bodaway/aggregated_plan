import { describe, it, expect } from "vitest";
import { buildPickerOptions } from "./gryzzly-picker-options";

const active = [
  { gryzzlyTaskId: "g1", name: "Dev", projectName: "Website" },
  { gryzzlyTaskId: "g2", name: "Specs", projectName: "Website" },
];

describe("buildPickerOptions", () => {
  it("returns active options grouped/sorted by project then name", () => {
    const opts = buildPickerOptions(active, null);
    expect(opts.map((o) => o.gryzzlyTaskId)).toEqual(["g1", "g2"]);
  });

  it("includes a stale assigned task not present in the active list", () => {
    const assigned = { gryzzlyTaskId: "g9", name: "Old", projectName: "Archived", stale: true };
    const opts = buildPickerOptions(active, assigned);
    const g9 = opts.find((o) => o.gryzzlyTaskId === "g9");
    expect(g9).toBeTruthy();
    expect(g9?.stale).toBe(true);
  });

  it("does not duplicate the assigned task when it is already active", () => {
    const assigned = { gryzzlyTaskId: "g1", name: "Dev", projectName: "Website", stale: false };
    const opts = buildPickerOptions(active, assigned);
    expect(opts.filter((o) => o.gryzzlyTaskId === "g1")).toHaveLength(1);
  });
});
