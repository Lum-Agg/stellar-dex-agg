'use client';

import { SwapCard } from '@/components/SwapCard';

export default function Home() {
  return (
    <div className="flex flex-col items-center">
      {/* Hero */}
      <div className="text-center mb-10">
        <h1 className="text-4xl font-bold mb-3 bg-gradient-to-r from-white to-gray-400 bg-clip-text text-transparent">
          Swap at the best rate
        </h1>
        <p className="text-gray-400 text-base max-w-md mx-auto">
          Split orders across multiple DEXes for optimal execution on Stellar
        </p>
      </div>

      {/* Swap */}
      <SwapCard />

      {/* Stats */}
      <div className="mt-16 grid grid-cols-3 gap-8 text-center w-full max-w-lg">
        <div>
          <div className="text-2xl font-bold text-white">4</div>
          <div className="text-xs text-gray-500 mt-1">DEX Sources</div>
        </div>
        <div>
          <div className="text-2xl font-bold text-white">500+</div>
          <div className="text-xs text-gray-500 mt-1">Pools</div>
        </div>
        <div>
          <div className="text-2xl font-bold text-white">&lt;1ms</div>
          <div className="text-xs text-gray-500 mt-1">Quote Speed</div>
        </div>
      </div>
    </div>
  );
}
