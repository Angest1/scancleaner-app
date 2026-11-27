import { HardDrive } from "lucide-react";
import { useState } from "react";
import type { DockerWslInfo } from "../../lib/ipc";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Collapse } from "../ui/Collapse";

export function DockerWslCard({ info }: { info?: DockerWslInfo }) {
  const [open, setOpen] = useState(false);
  if (!info?.dockerAvailable) return null;

  return (
    <Card className="flex flex-col gap-2">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2 font-semibold text-[13.5px] text-fg-primary">
          <HardDrive size={15} className="text-accent" />
          Compactación de disco virtual Docker / WSL (acción aparte)
        </div>
        <Button size="sm" onClick={() => setOpen((v) => !v)}>
          {open ? "Ocultar instrucciones" : "Ver instrucciones"}
        </Button>
      </div>
      <div className="text-[12.5px] text-fg-secondary">{info.vhdxNotice}</div>
      <Collapse open={open}>
        <div className="rounded-[8px] bg-canvas border border-border-subtle p-3 mt-1 flex flex-col gap-1 mono text-[12px] text-fg-secondary">
          {info.compactInstructions.map((step) => (
            <div key={step}>{step}</div>
          ))}
        </div>
      </Collapse>
    </Card>
  );
}
