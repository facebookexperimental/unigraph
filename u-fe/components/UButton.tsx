// Copyright (c) Meta Platforms, Inc. and affiliates.

import clsx from "clsx";
import { Button } from "./ui/button";

export default function UButton({
  children,
  className,
  onClick,
  disabled = false,
  variant = "default",
}: {
  children: React.ReactNode;
  className?: string;
  onClick?: () => void;
  disabled?: boolean;
  variant?: "default" | "outline" | "ghost" | "link";
}) {
  return (
    <Button
      className={clsx("cursor-pointer", className)}
      onClick={onClick}
      disabled={disabled}
      variant={variant}
    >
      {children}
    </Button>
  );
}
