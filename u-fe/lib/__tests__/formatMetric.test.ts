// Copyright (c) Meta Platforms, Inc. and affiliates.

import type { MetricFormat } from "@/__generated__/ts/MetricFormat";
import type { SizeConfig } from "@/__generated__/ts/SizeConfig";
import { expect, test } from "vitest";
import formatMetric from "../formatMetric";

test("Percent", () => {
  let format: MetricFormat = {
    Percent: {
      scaled_percentage: false,
    },
  };

  expect(formatMetric(1, format)).toBe("1%");
  expect(formatMetric(1.07, format)).toBe("1.07%");
  expect(formatMetric(0.01, format)).toBe("0.01%");
  expect(formatMetric(0.001, format)).toBe("0%");
  expect(formatMetric(1000, format)).toBe("1,000%");

  format = {
    Percent: {
      scaled_percentage: true,
    },
  };

  expect(formatMetric(1, format)).toBe("100%");
  expect(formatMetric(1.07, format)).toBe("107%");
  expect(formatMetric(0.01, format)).toBe("1%");
  expect(formatMetric(0.001, format)).toBe("0.1%");
  expect(formatMetric(1000, format)).toBe("100,000%");
});

const size: [SizeConfig, { [expected: number]: string }][] = [
  [
    { ForcekB: {} },
    {
      1: "0.00 kB",
      10: "0.01 kB",
      100: "0.10 kB",
      1000: "1.00 kB",
      10000: "10.00 kB",
      100000: "100.00 kB",
      1000000: "1,000.00 kB",
    },
  ],
  [
    { ForceMB: {} },
    {
      1: "0.00 MB",
      10: "0.00 MB",
      100: "0.00 MB",
      1000: "0.00 MB",
      10000: "0.01 MB",
      100000: "0.10 MB",
      1000000: "1.00 MB",
      10000000000: "10,000.00 MB",
    },
  ],
  [
    { ForceGB: {} },
    {
      1: "0.00 GB",
      10000000000: "10.00 GB",
      1000000000000: "1,000.00 GB",
    },
  ],
  [
    { ForceKiB: {} },
    {
      1000: "0.98 KiB",
      1024: "1.00 KiB",
    },
  ],
  [
    { ForceMiB: {} },
    {
      1000000: "0.95 MiB",
    },
  ],
  [
    { ForceGiB: {} },
    {
      1000000000000: "931.32 GiB",
    },
  ],
  [
    { VariableUnits: {} },
    {
      1: "1 byte",
      2: "2 bytes",
      10: "10 bytes",
      100: "100 bytes",
      1000: "1.00 kB",
      1024: "1.02 kB",
      10000: "10.00 kB",
      1000000: "1.00 MB",
      1000000000: "1.00 GB",
      10000000000000: "10.00 TB",
    },
  ],
];

for (const [config, data] of size) {
  const key = Object.keys(config)[0];
  for (const [valueS, expected] of Object.entries(data)) {
    const value = Number.parseFloat(valueS);
    test(`SizeConfig: ${key}. value: "${value}". expected: "${expected}"`, () => {
      const format: MetricFormat = {
        SizeBytes: {
          config,
        },
      };
      expect(formatMetric(value, format)).toBe(expected);
    });
  }
}

test("NumericBoolean", () => {
  const format: MetricFormat = {
    NumericBoolean: {},
  };

  expect(formatMetric(0, format)).toBe("False");
  expect(formatMetric(1, format)).toBe("True");
  expect(formatMetric(2, format)).toBe("2");
  expect(formatMetric(-1, format)).toBe("-1");
});

test("NumberWithVariablePrecision", () => {
  const format: MetricFormat = {
    NumberWithVariablePrecision: {
      min_precision: 1,
      max_precision: 3,
      use_delimiter: true,
    },
  };

  expect(formatMetric(1, format)).toBe("1.0");
  expect(formatMetric(1.07, format)).toBe("1.07");
  expect(formatMetric(0.01, format)).toBe("0.01");
  expect(formatMetric(0.001, format)).toBe("0.001");
  expect(formatMetric(1000, format)).toBe("1,000.0");
  expect(formatMetric(1.2345, format)).toBe("1.235");
});
