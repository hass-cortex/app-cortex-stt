interface CortexLogoProps {
	size?: number;
	className?: string;
}

/**
 * Cortex STT Server brand mark — Hex Server Crystal.
 *
 * 32×32 viewBox scaled from the 128×128 main icon, bars reduced from 7 to 5
 * for legibility at small sizes (16–32 px). Stroke color is locked to Ruby
 * Crimson #BE123C regardless of ambient text color — brand consistency wins
 * over theme integration.
 */
export function CortexLogo({ size = 24, className }: CortexLogoProps) {
	return (
		<svg
			width={size}
			height={size}
			viewBox="0 0 32 32"
			fill="none"
			xmlns="http://www.w3.org/2000/svg"
			className={className}
			role="img"
			aria-label="Cortex STT Server"
		>
			<title>Cortex STT Server</title>
			<path
				d="M16 3 L27.5 9.5 L27.5 22.5 L16 29 L4.5 22.5 L4.5 9.5 Z"
				stroke="#BE123C"
				strokeWidth="1.5"
				strokeLinejoin="round"
			/>
			<line x1="10" y1="14" x2="10" y2="18" stroke="#BE123C" strokeWidth="1.5" strokeLinecap="round" />
			<line x1="13" y1="12" x2="13" y2="20" stroke="#BE123C" strokeWidth="1.5" strokeLinecap="round" />
			<line x1="16" y1="10" x2="16" y2="22" stroke="#BE123C" strokeWidth="1.5" strokeLinecap="round" />
			<line x1="19" y1="12" x2="19" y2="20" stroke="#BE123C" strokeWidth="1.5" strokeLinecap="round" />
			<line x1="22" y1="14" x2="22" y2="18" stroke="#BE123C" strokeWidth="1.5" strokeLinecap="round" />
		</svg>
	);
}
