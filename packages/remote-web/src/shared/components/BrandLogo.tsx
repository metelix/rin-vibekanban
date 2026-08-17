interface BrandLogoProps {
  className?: string;
  alt?: string;
}

export function BrandLogo({
  className = "h-8 w-auto",
  alt = "RiN",
}: BrandLogoProps) {
  return <img src="/assets/rin-logo-horizontal.png" alt={alt} className={className} />;
}