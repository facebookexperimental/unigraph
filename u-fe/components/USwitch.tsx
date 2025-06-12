import { Label } from "./ui/label";
import { Switch } from "./ui/switch";

export default function USwitch({
  label,
  checked,
  onCheckedChange,
}: {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center space-x-2 cursor-pointer">
      <Switch
        id={`enable-${label}-column-switch`}
        className="cursor-pointer"
        checked={checked}
        onCheckedChange={onCheckedChange}
      />
      <Label
        htmlFor={`enable-${label}-column-switch`}
        className="ms-2 cursor-pointer"
      >
        {label}
      </Label>
    </div>
  );
}
