import { Component, type ErrorInfo, type ReactNode, useState } from "react";
import { H2, Pre } from "../Typography";
import UAlertDialog from "./UAlertDialog";
import { AlertDialogTitle } from "./ui/alert-dialog";
import { Button } from "./ui/button";

interface Props {
  children: ReactNode;
}

interface State {
  e: { error: Error; info: ErrorInfo } | null;
}

export default class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { e: null };
  }

  override componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    this.setState({ e: { error, info: errorInfo } });
  }

  override render() {
    if (this.state.e != null) {
      // You can render any custom fallback UI
      return (
        <>
          <H2 text="Something went wrong." />
          <ErrorModal error={this.state.e.error} info={this.state.e.info} />
        </>
      );
    }

    return this.props.children;
  }
}

function ErrorModal({ error, info }: { error: Error; info: ErrorInfo }) {
  const [open, setOpen] = useState(true);
  console.error(error);
  return (
    <UAlertDialog open={open} className="max-w-[1100px]">
      <div className="flex flex-col gap-2 w-full min-w-0">
        <AlertDialogTitle>Error</AlertDialogTitle>
        <Pre text={formatError(error, info)} />
        <div className="flex justify-end my-4">
          <Button
            type="submit"
            className="cursor-pointer"
            onClick={() => setOpen(false)}
          >
            Close
          </Button>
        </div>
      </div>
    </UAlertDialog>
  );
}

function formatError(error: Error, info: ErrorInfo): string {
  const errString = (() => {
    if (typeof error === "string") {
      return error;
    } else if (error instanceof Error) {
      return `${error.message}\n\nStack:\n${error.stack ?? "<no stack provided>"}`;
    } else {
      return JSON.stringify(error);
    }
  })();

  if (info.componentStack != null) {
    return `${errString}\n\nComponent Stack:\n${info.componentStack}`;
  }

  return errString;
}
