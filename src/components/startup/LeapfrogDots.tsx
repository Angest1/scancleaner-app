interface LeapfrogDotsProps {
  converging: boolean;
  reducedMotion: boolean;
}

const DOT_SLOTS = [0, 1, 2, 3, 4] as const;
const SLOT_SPACING_PX = 22;
const CENTER_INDEX = 2;

/**
 * 5-dot leapfrog activity indicator.
 * Dot 0 arcs continuously over dots 1..4 to the right end (~1.2s cycle) while dots 1..4 shift left
 * by one slot, using exclusively GPU-composited `transform` properties.
 * When `converging` is true, all 5 dots collapse smoothly into the center to morph into the tick circle.
 */
export function LeapfrogDots({ converging, reducedMotion }: LeapfrogDotsProps) {
  return (
    <div
      className="relative h-12 w-[112px] flex items-center justify-center"
      aria-hidden="true"
      data-testid="leapfrog-dots"
      data-converging={converging ? "true" : "false"}
    >
      {DOT_SLOTS.map((slot) => {
        const baseX = (slot - CENTER_INDEX) * SLOT_SPACING_PX;
        const convergeOffset = -baseX;
        const isLeaper = slot === 0;

        const animClass = converging
          ? "sc-dot-converging"
          : reducedMotion
            ? ""
            : isLeaper
              ? "sc-dot-leaper"
              : "sc-dot-shifter";

        return (
          <span
            key={slot}
            className="absolute top-1/2 left-1/2 -ml-[5.5px] -mt-[5.5px]"
            style={{
              transform: `translate3d(${baseX}px, 0, 0)`,
            }}
          >
            <span
              className={`block w-[11px] h-[11px] rounded-full ${
                isLeaper ? "bg-accent" : "bg-accent/70"
              } ${animClass}`}
              style={
                converging
                  ? {
                      transform: `translate3d(${convergeOffset}px, 0, 0) scale(1.65)`,
                      opacity: slot === CENTER_INDEX ? 1 : 0,
                    }
                  : reducedMotion
                    ? {
                        opacity: 0.45 + slot * 0.13,
                      }
                    : undefined
              }
            />
          </span>
        );
      })}
    </div>
  );
}
