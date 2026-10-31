import path from "node:path";

/** @type {import('next').NextConfig} */
const nextConfig = {
  async rewrites() {
    return [
      // Next generates /favicon.ico only from a binary app/favicon.ico. The mark
      // is an SVG, so point the conventional crawler path at it instead of 404ing.
      { source: "/favicon.ico", destination: "/icon.svg" },
    ];
  },
  images: {
    remotePatterns: [
      {
        protocol: "https",
        hostname: "images.unsplash.com",
        pathname: "/**",
      },
    ],
  },
  webpack(config) {
    config.resolve.alias = {
      ...(config.resolve.alias ?? {}),
      "@": path.resolve(process.cwd(), "src"),
    };

    return config;
  },
};

export default nextConfig;
