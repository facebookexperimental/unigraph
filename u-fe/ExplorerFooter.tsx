import { ArrowUpNarrowWide, List, Network, TreePine } from "lucide-react";
import UToggleButton from "./components/UToggleButton";
import { useGraphSettings } from "./context/GraphSettingsContext";

export default function ExplorerFooter() {
  const [graphSettings, setGraphSettings] = useGraphSettings();

  return (
    <div className="h-16 bg-card border-tw-full">
      <div className="flex gap-4 items-center mx-4 h-full">
        <UToggleButton
          tooltip="Show number of transitive children nodes"
          size="sm"
          selected={
            graphSettings.ui_settings?.columns?.show_transitive_count === true
          }
          onSelectedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_transitive_count: checked,
                },
              },
            });
          }}
        >
          <Network />
        </UToggleButton>
        <UToggleButton
          tooltip="Show number of parent nodes"
          size="sm"
          selected={
            graphSettings.ui_settings?.columns?.show_parents_count === true
          }
          onSelectedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                columns: {
                  ...graphSettings.ui_settings?.columns,
                  show_parents_count: checked,
                },
              },
            });
          }}
        >
          <ArrowUpNarrowWide />
        </UToggleButton>
        <UToggleButton
          tooltip="Show as a flat list"
          size="sm"
          selected={graphSettings.ui_settings?.show_as_a_flat_list === true}
          onSelectedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                show_as_a_flat_list: checked,
              },
            });
          }}
        >
          <List />
        </UToggleButton>
        <UToggleButton
          tooltip="Show as a dominator tree"
          size="sm"
          selected={graphSettings.ui_settings?.show_as_dominator_tree === true}
          onSelectedChange={(checked) => {
            setGraphSettings({
              ...graphSettings,
              ui_settings: {
                ...graphSettings.ui_settings,
                show_as_dominator_tree: checked,
              },
            });
          }}
        >
          <TreePine />
        </UToggleButton>
      </div>
    </div>
  );
}
