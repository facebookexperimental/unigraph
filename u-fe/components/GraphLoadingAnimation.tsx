// Copyright (c) Meta Platforms, Inc. and affiliates.

export function GraphLoadingAnimation() {
  return (
    <div className="unigraph-loading">
      <div className="unigraph-loading-orb" aria-hidden="true">
        <div className="unigraph-loading-aura" />
        <div className="unigraph-loading-orbit unigraph-loading-orbit-one">
          <div className="unigraph-loading-ring" />
        </div>
        <div className="unigraph-loading-core">
          <div className="unigraph-loading-plasma" />
          <div className="unigraph-loading-highlight" />
        </div>
        <div className="unigraph-loading-orbit unigraph-loading-orbit-two">
          <div className="unigraph-loading-ring" />
        </div>
      </div>
      <p role="status">Loading graph…</p>
    </div>
  );
}
