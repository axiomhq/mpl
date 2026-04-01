// Basic MPL interpreter evaluates PipeSteps

import type {
  Aggregate,
  Align,
  BucketBy,
  BucketSpec,
  Cmp,
  Filter,
  GroupBy,
  MapType,
  Mapping,
  Parameterized,
  PipeStep,
  RelativeTime,
  TagsType,
  TimeType,
} from "@axiomhq/mpl-lang";
import type { Series } from "./datasets";
import { loadSeries } from "./datasets";

function assert(condition: boolean, msg: string): void {
  if (!condition) throw new Error(msg);
}

function defined<T>(value: T | undefined | null, msg: string): asserts value is T {
  if (value == null) throw new Error(msg);
}

function unreachable(x: never): never {
  throw new Error(`Unreachable: ${x}`);
}

export interface InterpretResult {
  steps: Series[][];
  errors: (string | undefined)[];
}

function errMsg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function interpret(pipeSteps: PipeStep[]): InterpretResult {
  const steps: Series[][] = [];
  const errors: (string | undefined)[] = [];
  let series: Series[] = [];

  for (const step of pipeSteps) {
    const node = step.node;

    if ("Error" in node) {
      steps.push(clone(series));
      errors.push(node.Error);
      continue;
    }

    try {
      if ("Source" in node) {
        const ds = String(getParam(node.Source.metric_id.dataset));
        const metric = String(node.Source.metric_id.metric);
        series = loadSeries(ds, metric);
      } else if ("Sample" in node) {
        const rate = node.Sample;
        series = series.filter(() => Math.random() < rate);
      } else if ("Filter" in node) {
        series = applyFilter(series, [node.Filter]);
      } else if ("Aggregate" in node) {
        series = applyAggregate(series, node.Aggregate);
      } else if ("Compute" in node) {
        errors.push("Compute queries are not supported in the playground");
        steps.push(clone(series));
        continue;
      }
    } catch (e) {
      errors.push(errMsg(e));
      steps.push(clone(series));
      continue;
    }

    steps.push(clone(series));
    errors.push(undefined);
  }

  return { steps, errors };
}

function getParam<T>(p: Parameterized<T>): T {
  if ("Concrete" in p) return p.Concrete;
  throw new Error("Parameterized values are not supported in the playground");
}

function timeToSeconds(rt: RelativeTime): number {
  switch (rt.unit) {
    case "Millisecond":
      return rt.value / 1000;
    case "Second":
      return rt.value;
    case "Minute":
      return rt.value * 60;
    case "Hour":
      return rt.value * 60 * 60;
    case "Day":
      return rt.value * 24 * 60 * 60;
    case "Week":
      return rt.value * 7 * 24 * 60 * 60;
    case "Month":
      throw new Error("Month time unit is not supported");
    case "Year":
      throw new Error("Year time unit is not supported");
    default:
      return unreachable(rt.unit);
  }
}

function seriesName(tags: Record<string, string>): string {
  const entries = Object.entries(tags);
  if (entries.length === 0) return "(all)";
  return entries.map(([k, v]) => `${k}=${v}`).join(", ");
}

function requireTag(tags: Record<string, string>, key: string): string {
  const v = tags[key];
  defined(v, `Missing required tag: ${key}`);
  return v;
}

function clone(series: Series[]): Series[] {
  return series.map((s) => ({ ...s, values: [...s.values], timestamps: [...s.timestamps] }));
}

function evaluateCmp(tagVal: string, rhs: Cmp): boolean {
  if ("Eq" in rhs) return tagVal === String(getParam(rhs.Eq));
  if ("Ne" in rhs) return tagVal !== String(getParam(rhs.Ne));
  if ("Gt" in rhs) return Number(tagVal) > Number(String(getParam(rhs.Gt)));
  if ("Ge" in rhs) return Number(tagVal) >= Number(String(getParam(rhs.Ge)));
  if ("Lt" in rhs) return Number(tagVal) < Number(String(getParam(rhs.Lt)));
  if ("Le" in rhs) return Number(tagVal) <= Number(String(getParam(rhs.Le)));
  if ("RegEx" in rhs) {
    const r = getParam(rhs.RegEx);
    const pat = r instanceof RegExp ? r : new RegExp(String(r));
    return pat.test(tagVal);
  }
  if ("RegExNot" in rhs) {
    const r = getParam(rhs.RegExNot);
    const pat = r instanceof RegExp ? r : new RegExp(String(r));
    return !pat.test(tagVal);
  }
  if ("Is" in rhs) {
    switch (rhs.Is) {
      case "None":
        return tagVal === "";
      case "Bool":
        return tagVal === "true" || tagVal === "false";
      case "Int":
        return /^-?\d+$/.test(tagVal);
      case "Float":
        return /^-?\d+(\.\d+)?$/.test(tagVal);
      case "String":
        return true;
      default:
        return unreachable(rhs.Is);
    }
  }
  return unreachable(rhs);
}

function evaluateFilter(s: Series, filter: Filter): boolean {
  if ("Cmp" in filter) {
    const { field, rhs } = filter.Cmp;
    const tagVal = s.tags[field];
    if (tagVal == null) return false;
    return evaluateCmp(tagVal, rhs);
  }
  if ("And" in filter) return filter.And.every((f) => evaluateFilter(s, f));
  if ("Or" in filter) return filter.Or.some((f) => evaluateFilter(s, f));
  if ("Not" in filter) return !evaluateFilter(s, filter.Not);
  return unreachable(filter);
}

function filterFields(filter: Filter): string[] {
  if ("Cmp" in filter) return [filter.Cmp.field];
  if ("And" in filter) return filter.And.flatMap(filterFields);
  if ("Or" in filter) return filter.Or.flatMap(filterFields);
  if ("Not" in filter) return filterFields(filter.Not);
  return [];
}

function applyFilter(series: Series[], filters: Filter[]): Series[] {
  for (const f of filters) {
    for (const field of filterFields(f)) {
      assert(
        series.some((s) => field in s.tags),
        `Unknown tag: ${field}`,
      );
    }
  }
  return series.filter((s) => filters.every((f) => evaluateFilter(s, f)));
}

function reduceTagsPts(pts: number[], fn: TagsType): number {
  assert(pts.length > 0, `Cannot reduce empty points with ${fn}`);
  switch (fn) {
    case "Sum":
      return pts.reduce((a, b) => a + b, 0);
    case "Avg":
      return pts.reduce((a, b) => a + b, 0) / pts.length;
    case "Min":
      return Math.min(...pts);
    case "Max":
      return Math.max(...pts);
    case "Count":
      return pts.length;
    default:
      return unreachable(fn);
  }
}

type ReducibleTimeType = Exclude<TimeType, "Rate">;

function reduceTimePts(pts: number[], fn: ReducibleTimeType): number {
  assert(pts.length > 0, `Cannot reduce empty points with ${fn}`);
  switch (fn) {
    case "Sum":
      return pts.reduce((a, b) => a + b, 0);
    case "Avg":
      return pts.reduce((a, b) => a + b, 0) / pts.length;
    case "Min":
      return Math.min(...pts);
    case "Max":
      return Math.max(...pts);
    case "Count":
      return pts.length;
    case "Last":
      return pts[pts.length - 1];
    default:
      return unreachable(fn);
  }
}

function aggregateColumns(values: number[][], fn: TagsType): number[] {
  assert(values.length > 0, "Cannot aggregate empty series");
  const len = values[0].length;
  const result: number[] = [];
  for (let i = 0; i < len; i++) {
    const pts = values.map((v) => v[i]).filter((x) => x != null && !Number.isNaN(x));
    assert(pts.length > 0, `No valid points at column ${i} for ${fn}`);
    result.push(reduceTagsPts(pts, fn));
  }
  return result;
}

function applyGroup(series: Series[], groupBy: GroupBy): Series[] {
  assert(series.length > 0, "Cannot group empty series");
  const fn: TagsType = groupBy.function.Builtin;
  const tags = groupBy.tags;

  if (tags.length === 0) {
    const vals = aggregateColumns(
      series.map((s) => s.values),
      fn,
    );
    return [{ tags: {}, name: `${fn}(all)`, timestamps: [...series[0].timestamps], values: vals }];
  }

  const groups: Record<string, Series[]> = {};
  for (const s of series) {
    const key = tags.map((t) => requireTag(s.tags, t)).join("|");
    if (!groups[key]) groups[key] = [];
    groups[key].push(s);
  }

  return Object.values(groups).map((group) => {
    const vals = aggregateColumns(
      group.map((s) => s.values),
      fn,
    );
    const ts = group[0].timestamps;
    const newTags: Record<string, string> = {};
    for (const t of tags) newTags[t] = requireTag(group[0].tags, t);
    return { tags: newTags, name: seriesName(newTags), timestamps: [...ts], values: vals };
  });
}

function applyMap(series: Series[], mapping: Mapping): Series[] {
  const fn: MapType = mapping.function.Builtin;
  const arg = mapping.arg;

  return series.map((s) => {
    let v: number[];

    switch (fn) {
      case "IsLt":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x < arg ? 1 : 0));
        break;
      case "IsGt":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x > arg ? 1 : 0));
        break;
      case "IsEq":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x === arg ? 1 : 0));
        break;
      case "IsNe":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x !== arg ? 1 : 0));
        break;
      case "IsGe":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x >= arg ? 1 : 0));
        break;
      case "IsLe":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x <= arg ? 1 : 0));
        break;
      case "FilterLt":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x < arg ? x : NaN));
        break;
      case "FilterGt":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x > arg ? x : NaN));
        break;
      case "FilterEq":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x === arg ? x : NaN));
        break;
      case "FilterNe":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x !== arg ? x : NaN));
        break;
      case "FilterGe":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x >= arg ? x : NaN));
        break;
      case "FilterLe":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (x <= arg ? x : NaN));
        break;
      case "Rate":
        v = s.values.map((x, i) => {
          if (i === 0) return NaN;
          const dt = s.timestamps[i] - s.timestamps[i - 1];
          assert(dt > 0, `Non-positive time delta at index ${i}`);
          return (x - s.values[i - 1]) / dt;
        });
        break;
      case "Increase":
        v = s.values.map((x, i) => {
          if (i === 0) return NaN;
          return Math.max(0, x - s.values[i - 1]);
        });
        break;
      case "Abs":
        v = s.values.map(Math.abs);
        break;
      case "FillConst":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => (Number.isNaN(x) ? arg : x));
        break;
      case "FillPrev": {
        let prev: number | undefined;
        v = s.values.map((x) => {
          if (!Number.isNaN(x)) {
            prev = x;
            return x;
          }
          defined(prev, "FillPrev: no previous value available");
          return prev;
        });
        break;
      }
      case "Add":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => x + arg);
        break;
      case "Sub":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => x - arg);
        break;
      case "Mul":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => x * arg);
        break;
      case "Div":
        defined(arg, `Map function ${fn} requires an argument`);
        assert(arg !== 0, "Division by zero");
        v = s.values.map((x) => x / arg);
        break;
      case "Min":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => Math.min(x, arg));
        break;
      case "Max":
        defined(arg, `Map function ${fn} requires an argument`);
        v = s.values.map((x) => Math.max(x, arg));
        break;
      case "InterpolateLinear": {
        v = [...s.values];
        for (let i = 0; i < v.length; i++) {
          if (!Number.isNaN(v[i])) continue;
          let left = i - 1;
          while (left >= 0 && Number.isNaN(v[left])) left--;
          let right = i + 1;
          while (right < v.length && Number.isNaN(v[right])) right++;
          if (left >= 0 && right < v.length) {
            const denom = s.timestamps[right] - s.timestamps[left];
            assert(denom !== 0, "InterpolateLinear: duplicate timestamps");
            const frac = (s.timestamps[i] - s.timestamps[left]) / denom;
            v[i] = v[left] + frac * (v[right] - v[left]);
          } else {
            throw new Error(
              `InterpolateLinear: cannot interpolate at index ${i}, no bounding values`,
            );
          }
        }
        break;
      }
      default:
        unreachable(fn);
    }

    return { ...s, values: v };
  });
}

function windowPts(s: Series, t: number, windowSec: number): number[] {
  const pts: number[] = [];
  const end = t + windowSec;
  for (let i = 0; i < s.timestamps.length; i++) {
    if (s.timestamps[i] >= t && s.timestamps[i] < end && !Number.isNaN(s.values[i])) {
      pts.push(s.values[i]);
    }
  }
  return pts;
}

function applyAlign(series: Series[], align: Align): Series[] {
  const fn: TimeType = align.function.Builtin;
  const windowSec = timeToSeconds(getParam(align.time));

  return series.map((s) => {
    assert(s.timestamps.length > 0, "Cannot align series with no timestamps");
    const start = s.timestamps[0];
    const end = s.timestamps[s.timestamps.length - 1];
    const newTs: number[] = [];
    const newVals: number[] = [];

    for (let t = start; t <= end; t += windowSec) {
      newTs.push(t);
      const pts = windowPts(s, t, windowSec);

      if (fn === "Rate") {
        if (pts.length < 2) {
          newVals.push(NaN);
        } else {
          let increase = 0;
          for (let j = 1; j < pts.length; j++) {
            const delta = pts[j] - pts[j - 1];
            increase += delta < 0 ? pts[j] : delta;
          }
          newVals.push(increase / windowSec);
        }
      } else if (pts.length === 0) {
        newVals.push(NaN);
      } else {
        newVals.push(reduceTimePts(pts, fn));
      }
    }

    return { ...s, timestamps: newTs, values: newVals };
  });
}

function computePercentile(bucketVals: { le: number; val: number }[], p: number): number {
  assert(bucketVals.length > 0, "Cannot compute percentile with no buckets");
  const total = bucketVals[bucketVals.length - 1].val;
  assert(total !== 0, "Cannot compute percentile: total count is zero");
  const target = p * total;

  for (let i = 0; i < bucketVals.length; i++) {
    if (bucketVals[i].val >= target) {
      const prevVal = i > 0 ? bucketVals[i - 1].val : 0;
      const prevLe = i > 0 ? bucketVals[i - 1].le : 0;
      // val === prevVal is unreachable here: if they were equal and val >= target,
      // then prevVal >= target too, so the loop would have matched at i-1.
      assert(bucketVals[i].val !== prevVal, "Unexpected equal adjacent bucket values");
      const fraction = (target - prevVal) / (bucketVals[i].val - prevVal);
      return prevLe + fraction * (bucketVals[i].le - prevLe);
    }
  }
  throw new Error("Percentile computation failed: target exceeds all buckets");
}

function computeBucketSpec(
  spec: BucketSpec,
  groupSeries: Series[],
  byLe: { le: number; series: Series }[],
  t: number,
  windowSec: number,
): number {
  if (typeof spec === "object" && "Percentile" in spec) {
    const bucketVals = byLe.map(({ le, series: s }) => {
      const pts = windowPts(s, t, windowSec);
      if (pts.length === 0) return { le, val: 0 };
      return { le, val: pts.reduce((a, b) => a + b, 0) / pts.length };
    });
    if (bucketVals.every((b) => b.val === 0)) return NaN;
    return computePercentile(bucketVals, spec.Percentile);
  }

  const allPts: number[] = [];
  for (const s of groupSeries) {
    allPts.push(...windowPts(s, t, windowSec));
  }
  if (allPts.length === 0) return NaN;
  switch (spec) {
    case "Count":
      return allPts.length;
    case "Avg":
      return allPts.reduce((a, b) => a + b, 0) / allPts.length;
    case "Sum":
      return allPts.reduce((a, b) => a + b, 0);
    case "Min":
      return Math.min(...allPts);
    case "Max":
      return Math.max(...allPts);
    default:
      return unreachable(spec);
  }
}

function applyBucket(series: Series[], bucket: BucketBy): Series[] {
  assert(series.length > 0, "Cannot bucket empty series");
  const windowSec = timeToSeconds(getParam(bucket.time));
  const groupTags = bucket.tags;

  const groups: Record<string, Series[]> = {};
  for (const s of series) {
    const key = groupTags.map((t) => requireTag(s.tags, t)).join("|");
    if (!groups[key]) groups[key] = [];
    groups[key].push(s);
  }

  const result: Series[] = [];

  for (const groupSeries of Object.values(groups)) {
    const byLe: { le: number; series: Series }[] = [];
    for (const s of groupSeries) {
      const leStr = s.tags.le;
      if (leStr == null) continue;
      const le = parseFloat(leStr);
      assert(!Number.isNaN(le), `Invalid 'le' tag value: ${leStr}`);
      byLe.push({ le, series: s });
    }
    byLe.sort((a, b) => a.le - b.le);

    assert(groupSeries[0].timestamps.length > 0, "Cannot bucket series with no timestamps");
    const start = groupSeries[0].timestamps[0];
    const end = groupSeries[0].timestamps[groupSeries[0].timestamps.length - 1];
    const newTs: number[] = [];
    for (let t = start; t <= end; t += windowSec) newTs.push(t);

    const groupTagValues: Record<string, string> = {};
    for (const t of groupTags) groupTagValues[t] = requireTag(groupSeries[0].tags, t);

    for (const spec of bucket.spec) {
      const values = newTs.map((t) => computeBucketSpec(spec, groupSeries, byLe, t, windowSec));

      const specName =
        typeof spec === "object" && "Percentile" in spec
          ? `p${spec.Percentile * 100}`
          : String(spec);

      result.push({
        tags: { ...groupTagValues, spec: specName },
        name: `${seriesName(groupTagValues)}, ${specName}`,
        timestamps: newTs,
        values,
      });
    }
  }

  assert(result.length > 0, "Bucket produced no output series");
  return result;
}

function applyAggregate(series: Series[], agg: Aggregate): Series[] {
  if ("Map" in agg) return applyMap(series, agg.Map);
  if ("Align" in agg) return applyAlign(series, agg.Align);
  if ("GroupBy" in agg) return applyGroup(series, agg.GroupBy);
  if ("Bucket" in agg) return applyBucket(series, agg.Bucket);
  if ("As" in agg) return series.map((s) => ({ ...s, name: String(agg.As.name) }));
  return unreachable(agg);
}
