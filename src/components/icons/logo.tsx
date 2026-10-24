/** Owl's mark (assets/brand/mark.svg): a solid owl whose facial disc and wing
 * lines are cut out, so it takes the colour of the text around it. */
export const Logo = (props: React.SVGProps<SVGSVGElement>) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width={1200}
    height={1200}
    role="graphics-symbol img"
    fill="currentColor"
    viewBox="0 0 512 512"
    {...props}
  >
    <title>Owl</title>
    <path
      fillRule="evenodd"
      d="M118 60C148 104 178 128 214 138Q256 126 298 138C334 128 364 104 394 60C424 122 442 186 442 250C442 382 360 460 256 472C152 460 70 382 70 250C70 186 88 122 118 60ZM256 186C228 154 154 156 130 206C110 254 138 318 196 330C228 336 248 352 256 378C264 352 284 336 316 330C374 318 402 254 382 206C358 156 284 154 256 186ZM100 300C108 366 148 418 212 446C164 404 134 354 124 300ZM412 300C404 366 364 418 300 446C348 404 378 354 388 300Z"
    />
    <path
      fillRule="evenodd"
      d="M196 206A46 46 0 1 0 196.01 206ZM212 222A13 13 0 1 1 211.99 222Z M316 206A46 46 0 1 0 316.01 206ZM332 222A13 13 0 1 1 331.99 222Z"
    />
    <path d="M240 304L256 342L272 304Q256 294 240 304Z" />
  </svg>
);
