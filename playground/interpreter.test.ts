import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Aggregate, Filter, MapType, PipeStep, Source } from "@axiomhq/mpl-lang";
import type { Series } from "./datasets";

vi.mock("./datasets", () => ({
  loadSeries: vi.fn(),
}));

import { interpret } from "./interpreter";
import { loadSeries } from "./datasets";

const mockedLoadSeries = vi.mocked(loadSeries);

const span = { offset: 0, length: 0 };
const label = "";

function s(tags: Record<string, string>, timestamps: number[], values: number[]): Series {
  const name = Object.entries(tags)
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  return { tags, name, timestamps, values };
}

function src(dataset: string, metric: string): Source {
  return { metric_id: { dataset: { Concrete: dataset }, metric }, time: undefined };
}

function step(node: PipeStep["node"]): PipeStep {
  return { span, label, node, error: undefined };
}

function simpleQuery(
  dataset: string,
  metric: string,
  filters: Filter[] = [],
  aggregates: Aggregate[] = [],
): PipeStep[] {
  return [
    step({ Source: src(dataset, metric) }),
    ...filters.map((f) => step({ Filter: f })),
    ...aggregates.map((a) => step({ Aggregate: a })),
  ];
}

beforeEach(() => {
  mockedLoadSeries.mockReset();
});

describe("interpret - simple query", () => {
  it("loads series and returns initial step", () => {
    const data = [s({ host: "a" }, [0, 60], [10, 20])];
    mockedLoadSeries.mockReturnValue(data);

    const { steps } = interpret(simpleQuery("ds", "metric"));
    expect(steps).toHaveLength(1);
    expect(steps[0]).toHaveLength(1);
    expect(steps[0][0].values).toEqual([10, 20]);
  });

  it("returns error on parameterized dataset", () => {
    const source: Source = {
      metric_id: {
        dataset: {
          Param: {
            span: { offset: 0, length: 0 },
            param: { span: { offset: 0, length: 0 }, name: "ds", typ: "Dataset" },
          },
        },
        metric: "m",
      },
      time: undefined,
    };
    const { errors } = interpret([step({ Source: source })]);
    expect(errors.find(Boolean)).toContain("Parameterized");
  });
});

describe("filters", () => {
  it("Eq filter", () => {
    mockedLoadSeries.mockReturnValue([s({ host: "a" }, [0], [1]), s({ host: "b" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "host", rhs: { Eq: { Concrete: "a" } } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.host).toBe("a");
  });

  it("Ne filter", () => {
    mockedLoadSeries.mockReturnValue([s({ host: "a" }, [0], [1]), s({ host: "b" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "host", rhs: { Ne: { Concrete: "a" } } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.host).toBe("b");
  });

  it("Gt filter on numeric tags", () => {
    mockedLoadSeries.mockReturnValue([s({ code: "200" }, [0], [1]), s({ code: "500" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "code", rhs: { Gt: { Concrete: 300 } } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.code).toBe("500");
  });

  it("Ge filter", () => {
    mockedLoadSeries.mockReturnValue([s({ code: "200" }, [0], [1]), s({ code: "300" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "code", rhs: { Ge: { Concrete: 300 } } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.code).toBe("300");
  });

  it("Lt filter", () => {
    mockedLoadSeries.mockReturnValue([s({ code: "200" }, [0], [1]), s({ code: "500" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "code", rhs: { Lt: { Concrete: 300 } } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.code).toBe("200");
  });

  it("Le filter", () => {
    mockedLoadSeries.mockReturnValue([
      s({ code: "200" }, [0], [1]),
      s({ code: "300" }, [0], [2]),
      s({ code: "500" }, [0], [3]),
    ]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "code", rhs: { Le: { Concrete: 300 } } } }]),
    );
    expect(steps[1]).toHaveLength(2);
  });

  it("RegEx filter", () => {
    mockedLoadSeries.mockReturnValue([
      s({ path: "/api/v1" }, [0], [1]),
      s({ path: "/health" }, [0], [2]),
    ]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "path", rhs: { RegEx: { Concrete: /^\/api/ } } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.path).toBe("/api/v1");
  });

  it("RegExNot filter", () => {
    mockedLoadSeries.mockReturnValue([
      s({ path: "/api/v1" }, [0], [1]),
      s({ path: "/health" }, [0], [2]),
    ]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [
        { Cmp: { field: "path", rhs: { RegExNot: { Concrete: /^\/api/ } } } },
      ]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.path).toBe("/health");
  });

  it("Is None filter", () => {
    mockedLoadSeries.mockReturnValue([s({ val: "" }, [0], [1]), s({ val: "x" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "val", rhs: { Is: "None" } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.val).toBe("");
  });

  it("Is Bool filter", () => {
    mockedLoadSeries.mockReturnValue([s({ flag: "true" }, [0], [1]), s({ flag: "yes" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "flag", rhs: { Is: "Bool" } } }]),
    );
    expect(steps[1]).toHaveLength(1);
  });

  it("Is Int filter", () => {
    mockedLoadSeries.mockReturnValue([s({ n: "42" }, [0], [1]), s({ n: "3.14" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "n", rhs: { Is: "Int" } } }]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.n).toBe("42");
  });

  it("Is Float filter", () => {
    mockedLoadSeries.mockReturnValue([s({ n: "3.14" }, [0], [1]), s({ n: "abc" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "n", rhs: { Is: "Float" } } }]),
    );
    expect(steps[1]).toHaveLength(1);
  });

  it("Is String filter matches everything", () => {
    mockedLoadSeries.mockReturnValue([s({ x: "anything" }, [0], [1])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "x", rhs: { Is: "String" } } }]),
    );
    expect(steps[1]).toHaveLength(1);
  });

  it("missing tag returns error", () => {
    mockedLoadSeries.mockReturnValue([s({}, [0], [1])]);
    const { errors } = interpret(
      simpleQuery("ds", "m", [{ Cmp: { field: "missing", rhs: { Eq: { Concrete: "x" } } } }]),
    );
    expect(errors.find(Boolean)).toContain("Unknown tag: missing");
  });

  it("And filter", () => {
    mockedLoadSeries.mockReturnValue([
      s({ a: "1", b: "2" }, [0], [1]),
      s({ a: "1", b: "3" }, [0], [2]),
    ]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [
        {
          And: [
            { Cmp: { field: "a", rhs: { Eq: { Concrete: "1" } } } },
            { Cmp: { field: "b", rhs: { Eq: { Concrete: "2" } } } },
          ],
        },
      ]),
    );
    expect(steps[1]).toHaveLength(1);
  });

  it("Or filter", () => {
    mockedLoadSeries.mockReturnValue([
      s({ x: "a" }, [0], [1]),
      s({ x: "b" }, [0], [2]),
      s({ x: "c" }, [0], [3]),
    ]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [
        {
          Or: [
            { Cmp: { field: "x", rhs: { Eq: { Concrete: "a" } } } },
            { Cmp: { field: "x", rhs: { Eq: { Concrete: "c" } } } },
          ],
        },
      ]),
    );
    expect(steps[1]).toHaveLength(2);
  });

  it("Not filter", () => {
    mockedLoadSeries.mockReturnValue([s({ x: "a" }, [0], [1]), s({ x: "b" }, [0], [2])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [
        {
          Not: { Cmp: { field: "x", rhs: { Eq: { Concrete: "a" } } } },
        },
      ]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.x).toBe("b");
  });
});

describe("map operations", () => {
  const ts = [0, 60, 120];

  function mapQuery(fn: MapType, arg?: number): PipeStep[] {
    return simpleQuery("ds", "m", [], [{ Map: { function: { Builtin: fn }, arg } }]);
  }

  beforeEach(() => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, ts, [10, 20, 30])]);
  });

  it("IsLt", () => {
    const { steps } = interpret(mapQuery("IsLt", 20));
    expect(steps[1][0].values).toEqual([1, 0, 0]);
  });

  it("IsGt", () => {
    const { steps } = interpret(mapQuery("IsGt", 20));
    expect(steps[1][0].values).toEqual([0, 0, 1]);
  });

  it("IsEq", () => {
    const { steps } = interpret(mapQuery("IsEq", 20));
    expect(steps[1][0].values).toEqual([0, 1, 0]);
  });

  it("IsNe", () => {
    const { steps } = interpret(mapQuery("IsNe", 20));
    expect(steps[1][0].values).toEqual([1, 0, 1]);
  });

  it("IsGe", () => {
    const { steps } = interpret(mapQuery("IsGe", 20));
    expect(steps[1][0].values).toEqual([0, 1, 1]);
  });

  it("IsLe", () => {
    const { steps } = interpret(mapQuery("IsLe", 20));
    expect(steps[1][0].values).toEqual([1, 1, 0]);
  });

  it("FilterLt", () => {
    const { steps } = interpret(mapQuery("FilterLt", 25));
    expect(steps[1][0].values).toEqual([10, 20, NaN]);
  });

  it("FilterGt", () => {
    const { steps } = interpret(mapQuery("FilterGt", 15));
    expect(steps[1][0].values).toEqual([NaN, 20, 30]);
  });

  it("FilterEq", () => {
    const { steps } = interpret(mapQuery("FilterEq", 20));
    expect(steps[1][0].values).toEqual([NaN, 20, NaN]);
  });

  it("FilterNe", () => {
    const { steps } = interpret(mapQuery("FilterNe", 20));
    expect(steps[1][0].values).toEqual([10, NaN, 30]);
  });

  it("FilterGe", () => {
    const { steps } = interpret(mapQuery("FilterGe", 20));
    expect(steps[1][0].values).toEqual([NaN, 20, 30]);
  });

  it("FilterLe", () => {
    const { steps } = interpret(mapQuery("FilterLe", 20));
    expect(steps[1][0].values).toEqual([10, 20, NaN]);
  });

  it("Rate", () => {
    const { steps } = interpret(mapQuery("Rate"));
    const v = steps[1][0].values;
    expect(v[0]).toBeNaN();
    expect(v[1]).toBeCloseTo(10 / 60);
    expect(v[2]).toBeCloseTo(10 / 60);
  });

  it("Increase", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, ts, [10, 25, 20])]);
    const { steps } = interpret(mapQuery("Increase"));
    const v = steps[1][0].values;
    expect(v[0]).toBeNaN();
    expect(v[1]).toBe(15);
    expect(v[2]).toBe(0);
  });

  it("Abs", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, ts, [-5, 0, 3])]);
    const { steps } = interpret(mapQuery("Abs"));
    expect(steps[1][0].values).toEqual([5, 0, 3]);
  });

  it("FillConst", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, ts, [1, NaN, 3])]);
    const { steps } = interpret(mapQuery("FillConst", 99));
    expect(steps[1][0].values).toEqual([1, 99, 3]);
  });

  it("FillPrev", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, ts, [5, NaN, NaN])]);
    const { steps } = interpret(mapQuery("FillPrev"));
    expect(steps[1][0].values).toEqual([5, 5, 5]);
  });

  it("FillPrev throws if no previous value", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, ts, [NaN, 1, 2])]);
    const { errors } = interpret(mapQuery("FillPrev"));
    expect(errors.find(Boolean)).toContain("no previous value");
  });

  it("Add", () => {
    const { steps } = interpret(mapQuery("Add", 5));
    expect(steps[1][0].values).toEqual([15, 25, 35]);
  });

  it("Sub", () => {
    const { steps } = interpret(mapQuery("Sub", 5));
    expect(steps[1][0].values).toEqual([5, 15, 25]);
  });

  it("Mul", () => {
    const { steps } = interpret(mapQuery("Mul", 2));
    expect(steps[1][0].values).toEqual([20, 40, 60]);
  });

  it("Div", () => {
    const { steps } = interpret(mapQuery("Div", 10));
    expect(steps[1][0].values).toEqual([1, 2, 3]);
  });

  it("Div by zero throws", () => {
    const { errors } = interpret(mapQuery("Div", 0));
    expect(errors.find(Boolean)).toContain("Division by zero");
  });

  it("Min", () => {
    const { steps } = interpret(mapQuery("Min", 15));
    expect(steps[1][0].values).toEqual([10, 15, 15]);
  });

  it("Max", () => {
    const { steps } = interpret(mapQuery("Max", 25));
    expect(steps[1][0].values).toEqual([25, 25, 30]);
  });

  it("InterpolateLinear", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 60, 120], [10, NaN, 30])]);
    const { steps } = interpret(mapQuery("InterpolateLinear"));
    expect(steps[1][0].values).toEqual([10, 20, 30]);
  });

  it("InterpolateLinear throws with no bounding values", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 60], [NaN, 10])]);
    const { errors } = interpret(mapQuery("InterpolateLinear"));
    expect(errors.find(Boolean)).toContain("no bounding values");
  });

  it("map function without required arg throws", () => {
    const { errors } = interpret(mapQuery("Add"));
    expect(errors.find(Boolean)).toContain("requires an argument");
  });
});

describe("group operations", () => {
  it("group by tag using Sum", () => {
    mockedLoadSeries.mockReturnValue([
      s({ region: "us" }, [0, 60], [10, 20]),
      s({ region: "us" }, [0, 60], [30, 40]),
      s({ region: "eu" }, [0, 60], [5, 15]),
    ]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            GroupBy: {
              span: { offset: 0, length: 0 },
              function: { Builtin: "Sum" },
              tags: ["region"],
            },
          },
        ],
      ),
    );
    expect(steps[1]).toHaveLength(2);
    const us = steps[1].find((s) => s.tags.region === "us");
    expect(us?.values).toEqual([40, 60]);
  });

  it("group all using Avg", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0], [10]), s({ h: "b" }, [0], [20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [{ GroupBy: { span: { offset: 0, length: 0 }, function: { Builtin: "Avg" }, tags: [] } }],
      ),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].values).toEqual([15]);
  });

  it("group using Count", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0], [10]), s({ h: "b" }, [0], [20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [{ GroupBy: { span: { offset: 0, length: 0 }, function: { Builtin: "Count" }, tags: [] } }],
      ),
    );
    expect(steps[1][0].values).toEqual([2]);
  });

  it("group using Min", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0], [10]), s({ h: "b" }, [0], [20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [{ GroupBy: { span: { offset: 0, length: 0 }, function: { Builtin: "Min" }, tags: [] } }],
      ),
    );
    expect(steps[1][0].values).toEqual([10]);
  });

  it("group using Max", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0], [10]), s({ h: "b" }, [0], [20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [{ GroupBy: { span: { offset: 0, length: 0 }, function: { Builtin: "Max" }, tags: [] } }],
      ),
    );
    expect(steps[1][0].values).toEqual([20]);
  });

  it("group with missing tag throws", () => {
    mockedLoadSeries.mockReturnValue([s({}, [0], [1])]);
    const { errors } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            GroupBy: {
              span: { offset: 0, length: 0 },
              function: { Builtin: "Sum" },
              tags: ["missing"],
            },
          },
        ],
      ),
    );
    expect(errors.find(Boolean)).toContain("Missing required tag");
  });
});

describe("align operations", () => {
  it("align using Sum", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 30, 60, 90], [1, 2, 3, 4])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Sum" },
              time: { Concrete: { value: 60, unit: "Second" } },
            },
          },
        ],
      ),
    );
    expect(steps[1][0].values).toEqual([3, 7]);
  });

  it("align using Avg", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 30, 60, 90], [10, 20, 30, 40])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Avg" },
              time: { Concrete: { value: 1, unit: "Minute" } },
            },
          },
        ],
      ),
    );
    expect(steps[1][0].values).toEqual([15, 35]);
  });

  it("align using Rate (prom::rate)", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 30, 60, 90], [0, 100, 200, 400])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Rate" },
              time: { Concrete: { value: 60, unit: "Second" } },
            },
          },
        ],
      ),
    );
    const v = steps[1][0].values;
    expect(v[0]).toBeCloseTo(100 / 60);
    expect(v[1]).toBeCloseTo(200 / 60);
  });

  it("align Rate with counter reset", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 30, 60, 90], [100, 50, 60, 70])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Rate" },
              time: { Concrete: { value: 60, unit: "Second" } },
            },
          },
        ],
      ),
    );
    // Window [0,60): 100 -> 50 (reset: +50), so increase = 50
    expect(steps[1][0].values[0]).toBeCloseTo(50 / 60);
  });

  it("align using Last", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 30, 60, 90], [1, 2, 3, 4])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Last" },
              time: { Concrete: { value: 60, unit: "Second" } },
            },
          },
        ],
      ),
    );
    expect(steps[1][0].values).toEqual([2, 4]);
  });

  it("align with Month time unit throws", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0], [1])]);
    const { errors } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Sum" },
              time: { Concrete: { value: 1, unit: "Month" } },
            },
          },
        ],
      ),
    );
    expect(errors.find(Boolean)).toContain("Month time unit is not supported");
  });
});

describe("as operation", () => {
  it("renames series", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0], [1])]);
    const { steps } = interpret(simpleQuery("ds", "m", [], [{ As: { name: "new_name" } }]));
    expect(steps[1][0].name).toBe("new_name");
  });
});

describe("compute query", () => {
  it("returns not supported error", () => {
    const { errors } = interpret([
      step({ Compute: { left: [], right: [], name: "x", op: { Builtin: "Div" } } }),
    ]);
    expect(errors.find(Boolean)).toContain("Compute queries are not supported");
  });
});

describe("bucket operations", () => {
  it("bucket with percentile", () => {
    mockedLoadSeries.mockReturnValue([
      s({ handler: "api", le: "0.1" }, [0, 60], [10, 20]),
      s({ handler: "api", le: "0.5" }, [0, 60], [50, 80]),
      s({ handler: "api", le: "1.0" }, [0, 60], [100, 100]),
    ]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: [{ Percentile: 0.5 }],
            },
          },
        ],
      ),
    );
    const last = steps[steps.length - 1];
    expect(last).toHaveLength(1);
    expect(last[0].tags.handler).toBe("api");
    expect(last[0].tags.spec).toBe("p50");
  });

  it("bucket with Count spec", () => {
    mockedLoadSeries.mockReturnValue([s({ handler: "api", le: "0.5" }, [0, 60], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: ["Count"],
            },
          },
        ],
      ),
    );
    expect(steps[steps.length - 1][0].tags.spec).toBe("Count");
  });

  it("bucket with Sum spec", () => {
    mockedLoadSeries.mockReturnValue([s({ handler: "api", le: "0.5" }, [0, 60], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: ["Sum"],
            },
          },
        ],
      ),
    );
    expect(steps[steps.length - 1][0].values[0]).toBe(30);
  });

  it("bucket with Avg spec", () => {
    mockedLoadSeries.mockReturnValue([s({ handler: "api", le: "0.5" }, [0, 60], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: ["Avg"],
            },
          },
        ],
      ),
    );
    expect(steps[steps.length - 1][0].values[0]).toBe(15);
  });

  it("bucket with Min spec", () => {
    mockedLoadSeries.mockReturnValue([s({ handler: "api", le: "0.5" }, [0, 60], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: ["Min"],
            },
          },
        ],
      ),
    );
    expect(steps[steps.length - 1][0].values[0]).toBe(10);
  });

  it("bucket with Max spec", () => {
    mockedLoadSeries.mockReturnValue([s({ handler: "api", le: "0.5" }, [0, 60], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: ["Max"],
            },
          },
        ],
      ),
    );
    expect(steps[steps.length - 1][0].values[0]).toBe(20);
  });
});

describe("time unit conversions", () => {
  it("Millisecond", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 1], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Sum" },
              time: { Concrete: { value: 2000, unit: "Millisecond" } },
            },
          },
        ],
      ),
    );
    expect(steps[1][0].timestamps).toEqual([0]);
  });

  it("Hour", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 1800, 3600, 5400], [1, 2, 3, 4])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: { function: { Builtin: "Sum" }, time: { Concrete: { value: 1, unit: "Hour" } } },
          },
        ],
      ),
    );
    expect(steps[1][0].values).toEqual([3, 7]);
  });

  it("Year throws", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0], [1])]);
    const { errors } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Sum" },
              time: { Concrete: { value: 1, unit: "Year" } },
            },
          },
        ],
      ),
    );
    expect(errors.find(Boolean)).toContain("Year time unit is not supported");
  });
});

describe("step cloning", () => {
  it("mutating returned series does not affect earlier steps", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 60], [10, 20])]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [], [{ Map: { function: { Builtin: "Mul" }, arg: 2 } }]),
    );
    expect(steps[0][0].values).toEqual([10, 20]);
    expect(steps[1][0].values).toEqual([20, 40]);
    steps[1][0].values[0] = 999;
    expect(steps[0][0].values[0]).toBe(10);
  });
});

describe("coverage gaps", () => {
  it("Day time unit", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 43200, 86400, 129600], [1, 2, 3, 4])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: { function: { Builtin: "Sum" }, time: { Concrete: { value: 1, unit: "Day" } } },
          },
        ],
      ),
    );
    expect(steps[1][0].values).toEqual([3, 7]);
  });

  it("Week time unit", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 302400, 604800], [1, 2, 3])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: { function: { Builtin: "Sum" }, time: { Concrete: { value: 1, unit: "Week" } } },
          },
        ],
      ),
    );
    expect(steps[1][0].values).toEqual([3, 3]);
  });

  it("RegEx filter with string pattern (not RegExp object)", () => {
    mockedLoadSeries.mockReturnValue([
      s({ path: "/api" }, [0], [1]),
      s({ path: "/health" }, [0], [2]),
    ]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [
        { Cmp: { field: "path", rhs: { RegEx: { Concrete: "^/api" as unknown as RegExp } } } },
      ]),
    );
    expect(steps[1]).toHaveLength(1);
  });

  it("RegExNot filter with string pattern", () => {
    mockedLoadSeries.mockReturnValue([
      s({ path: "/api" }, [0], [1]),
      s({ path: "/health" }, [0], [2]),
    ]);
    const { steps } = interpret(
      simpleQuery("ds", "m", [
        { Cmp: { field: "path", rhs: { RegExNot: { Concrete: "^/api" as unknown as RegExp } } } },
      ]),
    );
    expect(steps[1]).toHaveLength(1);
    expect(steps[1][0].tags.path).toBe("/health");
  });

  it("align with empty window produces NaN", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 200], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Sum" },
              time: { Concrete: { value: 60, unit: "Second" } },
            },
          },
        ],
      ),
    );
    // Windows at 0, 60, 120, 180 — window 60 and 120 have no points
    expect(steps[1][0].values[0]).toBe(10);
    expect(steps[1][0].values[1]).toBeNaN();
  });

  it("align Rate with fewer than 2 points produces NaN", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 200], [10, 20])]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Align: {
              function: { Builtin: "Rate" },
              time: { Concrete: { value: 60, unit: "Second" } },
            },
          },
        ],
      ),
    );
    // Window at 0 has 1 point, window at 60 has 0 points — both NaN
    expect(steps[1][0].values[0]).toBeNaN();
  });

  it("align using Min, Max, Count, Last via reduceTimePts", () => {
    mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 30, 60, 90], [5, 15, 25, 35])]);

    for (const fn of ["Min", "Max", "Count", "Last"] as const) {
      mockedLoadSeries.mockReturnValue([s({ h: "a" }, [0, 30, 60, 90], [5, 15, 25, 35])]);
      const { steps } = interpret(
        simpleQuery(
          "ds",
          "m",
          [],
          [
            {
              Align: {
                function: { Builtin: fn },
                time: { Concrete: { value: 60, unit: "Second" } },
              },
            },
          ],
        ),
      );
      const v = steps[1][0].values;
      switch (fn) {
        case "Min":
          expect(v).toEqual([5, 25]);
          break;
        case "Max":
          expect(v).toEqual([15, 35]);
          break;
        case "Count":
          expect(v).toEqual([2, 2]);
          break;
        case "Last":
          expect(v).toEqual([15, 35]);
          break;
      }
    }
  });

  it("computePercentile at first bucket (i === 0)", () => {
    mockedLoadSeries.mockReturnValue([
      s({ handler: "api", le: "0.1" }, [0, 60], [90, 90]),
      s({ handler: "api", le: "1.0" }, [0, 60], [100, 100]),
    ]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: [{ Percentile: 0.5 }],
            },
          },
        ],
      ),
    );
    // 50th percentile should fall in the first bucket range
    expect(steps[steps.length - 1][0].values[0]).toBeLessThanOrEqual(0.1);
  });

  it("bucket with series missing le tag (skipped)", () => {
    mockedLoadSeries.mockReturnValue([
      s({ handler: "api", le: "0.5" }, [0, 60], [50, 80]),
      s({ handler: "api" }, [0, 60], [10, 20]), // no le tag — should be skipped for percentile
    ]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [
          {
            Bucket: {
              span: { offset: 0, length: 0 },
              function: "Histogram",
              time: { Concrete: { value: 120, unit: "Second" } },
              tags: ["handler"],
              spec: ["Count"],
            },
          },
        ],
      ),
    );
    expect(steps[steps.length - 1]).toHaveLength(1);
  });
});

describe("InterpolateLinear consecutive NaN gaps", () => {
  it("interpolates across multiple consecutive NaN values", () => {
    mockedLoadSeries.mockReturnValue([
      s({ h: "a" }, [0, 60, 120, 180, 240], [10, NaN, NaN, NaN, 50]),
    ]);
    const { steps } = interpret(
      simpleQuery(
        "ds",
        "m",
        [],
        [{ Map: { function: { Builtin: "InterpolateLinear" }, arg: undefined } }],
      ),
    );
    const v = steps[1][0].values;
    expect(v[0]).toBe(10);
    expect(v[1]).toBe(20);
    expect(v[2]).toBe(30);
    expect(v[3]).toBe(40);
    expect(v[4]).toBe(50);
  });
});
