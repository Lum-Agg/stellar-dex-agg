import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  reactStrictMode: true,
  output: 'export',
  images: {
    unoptimized: true,
    remotePatterns: [
      {
        protocol: 'https',
        hostname: 'api.lumagg.xyz',
        pathname: '/logos/**',
      },
    ],
  },
  transpilePackages: ['@creit.tech/stellar-wallets-kit'],
  webpack: (config, { isServer }) => {
    // stellar-base probes sodium-native in its Node path before falling back to
    // the browser signer. Keep unrelated webpack warnings visible.
    config.ignoreWarnings = [
      ...(config.ignoreWarnings ?? []),
      {
        module: /node_modules\/(?:require-addon|sodium-native)\//,
        message: /Critical dependency/,
      },
    ];

    if (!isServer) {
      config.resolve.fallback = {
        ...config.resolve.fallback,
        crypto: false,
        stream: false,
        buffer: false,
        http: false,
        https: false,
        os: false,
        url: false,
        zlib: false,
        fs: false,
        net: false,
        tls: false,
      };
    }
    return config;
  },
};

export default nextConfig;
