import type { NonTreeColumnDefinition } from "@/tree_table/TreeTable";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import { Collapsible, CollapsibleTrigger } from "../components/ui/collapsible";
import { Label } from "../components/ui/label";
import { Switch } from "../components/ui/switch";
import { useGraphTreeTableColumns } from "../context/GraphTreeTableColumnsContext";
import { SidebarPanel, SidebarPanelHeader } from "./SidebarPanel";

export default function ColumnsPanel() {
  const { columnDefinitions, setColumnDefinitions } =
    useGraphTreeTableColumns();

  const cards = Object.entries(columnDefinitions.columns).map(
    ([columnID, definition]) => {
      return (
        <ColumnCard
          key={columnID}
          columnID={columnID}
          definition={definition}
          onUpdateDefinition={(newDefinition) => {
            setColumnDefinitions({
              ...columnDefinitions,
              columns: {
                ...columnDefinitions.columns,
                [columnID]: newDefinition,
              },
            });
          }}
        />
      );
    },
  );

  return (
    <SidebarPanel>
      <SidebarPanelHeader>Columns</SidebarPanelHeader>
      <div className="flex flex-col gap-2">{cards}</div>
    </SidebarPanel>
  );
}

function ColumnCard({
  columnID,
  definition,
  onUpdateDefinition,
}: {
  columnID: string;
  definition: NonTreeColumnDefinition;
  onUpdateDefinition: (definition: NonTreeColumnDefinition) => void;
}) {
  const [isOpen, setIsOpen] = useState(true);

  return (
    <Collapsible
      open={isOpen}
      onOpenChange={setIsOpen}
      className="flex w-full flex-col gap-2 bg-accent rounded-lg py-1"
    >
      <CollapsibleTrigger className="cursor-pointer flex justify-between mx-2 my-1">
        <p className="px-2">{definition.label}</p>
        {isOpen ? <ChevronDown /> : <ChevronRight />}
      </CollapsibleTrigger>
      {isOpen && (
        <ColumnCardContent
          columnID={columnID}
          definition={definition}
          onUpdateDefinition={onUpdateDefinition}
        />
      )}
    </Collapsible>
  );
}

function ColumnCardContent({
  columnID,
  definition,
  onUpdateDefinition,
}: {
  columnID: string;
  definition: NonTreeColumnDefinition;
  onUpdateDefinition: (definition: NonTreeColumnDefinition) => void;
}) {
  return (
    <div className="flex flex-col gap-2 bg-sidebar mx-1 p-3 rounded-lg">
      <div className="flex items-center space-x-2 cursor-pointer">
        <Switch
          id={`enable-${columnID}-column-switch`}
          className="cursor-pointer"
          checked={!definition.isHidden}
          onCheckedChange={(checked) => {
            if (onUpdateDefinition) {
              onUpdateDefinition({
                ...definition,
                isHidden: !checked,
              });
            }
          }}
        />
        <Label
          htmlFor={`enable-${columnID}-column-switch`}
          className="ms-2 cursor-pointer"
        >
          Show
        </Label>
      </div>
    </div>
  );
}
