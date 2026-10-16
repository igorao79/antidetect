export const Logo = (props: React.SVGProps<SVGSVGElement>) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width={1200}
    height={1200}
    role="graphics-symbol img"
    fill="none"
    stroke="currentColor"
    viewBox="0 0 900 900"
    {...props}
  >
    <title>Owl</title>
    <path
      d="M170 120 L340 250 Q450 222 560 250 L730 120 C755 250 765 345 765 445 C765 650 625 800 450 830 C275 800 135 650 135 445 C135 345 145 250 170 120 Z"
      strokeWidth={56}
      strokeLinejoin="round"
    />
    <g strokeWidth={34} strokeLinecap="round" strokeLinejoin="round">
      <path d="M205 365 Q330 300 450 400 Q570 300 695 365" />
      <circle cx="325" cy="480" r="92" />
      <circle cx="575" cy="480" r="92" />
      <path d="M418 545 L450 600 L482 545" />
      <path d="M230 590 C262 680 322 742 398 776" />
      <path d="M670 590 C638 680 578 742 502 776" />
    </g>
    <g fill="currentColor" stroke="none">
      <circle cx="325" cy="485" r="38" />
      <circle cx="575" cy="485" r="38" />
    </g>
  </svg>
);
