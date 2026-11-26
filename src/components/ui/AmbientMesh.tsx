import { useEffect, useRef } from "react";

/**
 * Ambient atmospheric gradient mesh with GPU-composited mouse parallax.
 * Uses direct style updates inside requestAnimationFrame (zero React re-renders).
 */
export function AmbientMesh() {
  const layer1Ref = useRef<HTMLDivElement>(null);
  const layer2Ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced) return;

    let targetX = 0;
    let targetY = 0;
    let currentX = 0;
    let currentY = 0;
    let rafId = 0;

    const tick = () => {
      currentX += (targetX - currentX) * 0.08;
      currentY += (targetY - currentY) * 0.08;

      if (layer1Ref.current) {
        layer1Ref.current.style.transform = `translate3d(${currentX * 22}px, ${currentY * 18}px, 0)`;
      }
      if (layer2Ref.current) {
        layer2Ref.current.style.transform = `translate3d(${-currentX * 18}px, ${-currentY * 16}px, 0)`;
      }

      if (Math.abs(targetX - currentX) > 0.001 || Math.abs(targetY - currentY) > 0.001) {
        rafId = requestAnimationFrame(tick);
      } else {
        rafId = 0;
      }
    };

    const onPointerMove = (e: PointerEvent) => {
      targetX = e.clientX / window.innerWidth - 0.5;
      targetY = e.clientY / window.innerHeight - 0.5;
      if (!rafId) rafId = requestAnimationFrame(tick);
    };

    window.addEventListener("pointermove", onPointerMove, { passive: true });
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      if (rafId) cancelAnimationFrame(rafId);
    };
  }, []);

  return (
    <div
      aria-hidden="true"
      className="fixed inset-0 z-0 overflow-hidden pointer-events-none select-none"
    >
      <div ref={layer1Ref} className="absolute inset-[-60px] will-change-transform">
        <div
          className="absolute -top-24 left-[8%] w-[520px] h-[520px] rounded-full blur-[96px]"
          style={{ background: "radial-gradient(circle, var(--mesh-1) 0%, transparent 70%)" }}
        />
        <div
          className="absolute top-[10%] right-[6%] w-[480px] h-[480px] rounded-full blur-[96px]"
          style={{ background: "radial-gradient(circle, var(--mesh-2) 0%, transparent 70%)" }}
        />
      </div>

      <div ref={layer2Ref} className="absolute inset-[-60px] will-change-transform">
        <div
          className="absolute bottom-[-120px] left-[28%] w-[560px] h-[440px] rounded-full blur-[108px]"
          style={{ background: "radial-gradient(circle, var(--mesh-3) 0%, transparent 70%)" }}
        />
        <div
          className="absolute bottom-[4%] right-[14%] w-[400px] h-[380px] rounded-full blur-[96px]"
          style={{ background: "radial-gradient(circle, var(--mesh-4) 0%, transparent 70%)" }}
        />
      </div>
    </div>
  );
}
