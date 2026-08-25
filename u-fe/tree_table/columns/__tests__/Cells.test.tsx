// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Slot } from "@radix-ui/react-slot";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, test } from "vitest";
import {
  DeltaMetricCell,
  MetricCell,
  MissingMetric,
  NO_PRECISION_FORMAT,
} from "../Cells";

/// A Radix `asChild` trigger — `UHoverCard asChild`, `UTooltip` — renders a
/// `Slot`, which clones the child and merges its own `ref`, `className` and
/// pointer handlers into the child's props. A cell that destructures only what
/// it uses drops all of that, and the trigger silently does nothing: no error,
/// no missing element, just a hovercard that never opens.
///
/// `renderToStaticMarkup` cannot show event handlers, but `data-*` attributes
/// travel the identical path, so an injected attribute reaching the `<span>`
/// is the property under test.
test("cells forward slot props to their span", () => {
  const cells = [
    [
      "MetricCell",
      <MetricCell key="m" value={1234} format={NO_PRECISION_FORMAT} />,
    ],
    [
      "DeltaMetricCell",
      <DeltaMetricCell key="d" value={-5} format={NO_PRECISION_FORMAT} />,
    ],
    ["MissingMetric", <MissingMetric key="x" />],
  ] as const;

  const table = cells
    .map(([label, cell]) => {
      const html = renderToStaticMarkup(
        <Slot data-slot-reached="yes" className="injected-by-slot">
          {cell}
        </Slot>,
      );
      expect(html, `${label} must receive slot props`).toContain(
        'data-slot-reached="yes"',
      );
      expect(html, `${label} must keep the slot's className`).toContain(
        "injected-by-slot",
      );
      expect(html, `${label} must keep its own classes`).toContain(
        "tabular-nums",
      );
      return `${label.padEnd(16)} ${html}`;
    })
    .join("\n");

  expect(table).toMatchInlineSnapshot(`
    "MetricCell       <span data-slot-reached="yes" class="px-4 text-right tabular-nums w-full whitespace-nowrap injected-by-slot">1,234</span>
    DeltaMetricCell  <span data-slot-reached="yes" class="px-4 text-right tabular-nums w-full whitespace-nowrap font-semibold text-green-600 injected-by-slot">-5</span>
    MissingMetric    <span data-slot-reached="yes" class="px-4 text-right tabular-nums w-full whitespace-nowrap injected-by-slot">-</span>"
  `);
});

/// The cells still render what they always did when nothing is injected.
test("cells render their value", () => {
  const rendered = [
    renderToStaticMarkup(
      <MetricCell value={1234} format={NO_PRECISION_FORMAT} />,
    ),
    renderToStaticMarkup(
      <MetricCell value={1234} format={NO_PRECISION_FORMAT} muted />,
    ),
    renderToStaticMarkup(
      <DeltaMetricCell value={5} format={NO_PRECISION_FORMAT} />,
    ),
    renderToStaticMarkup(
      <DeltaMetricCell value={-5} format={NO_PRECISION_FORMAT} />,
    ),
    renderToStaticMarkup(
      <DeltaMetricCell value={0} format={NO_PRECISION_FORMAT} />,
    ),
    renderToStaticMarkup(<MissingMetric />),
  ].join("\n");

  expect(rendered).toMatchInlineSnapshot(`
    "<span class="px-4 text-right tabular-nums w-full whitespace-nowrap">1,234</span>
    <span class="px-4 text-right tabular-nums w-full whitespace-nowrap text-muted-foreground">1,234</span>
    <span class="px-4 text-right tabular-nums w-full whitespace-nowrap font-semibold text-red-600">+5</span>
    <span class="px-4 text-right tabular-nums w-full whitespace-nowrap font-semibold text-green-600">-5</span>
    <span class="px-4 text-right tabular-nums w-full whitespace-nowrap">-</span>
    <span class="px-4 text-right tabular-nums w-full whitespace-nowrap">-</span>"
  `);
});
