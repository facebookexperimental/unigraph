// Copyright (c) Meta Platforms, Inc. and affiliates.

import { Links, Meta, Outlet, Scripts, ScrollRestoration } from "react-router";
import { createFetchTransport, RpcProvider } from "../api/rpc";
import "../input.css";

const transport = createFetchTransport("/api/rpc");

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <meta charSet="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <Meta />
        <Links />
        <title>Unigraph</title>
      </head>
      <body>
        {children}
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function Root() {
  return (
    <RpcProvider transport={transport}>
      <Outlet />
    </RpcProvider>
  );
}

export function HydrateFallback() {
  return (
    <div className="h-screen flex items-center justify-center">Loading…</div>
  );
}
