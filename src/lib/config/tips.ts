export const tips = [
  "Use a bed to skip the night and set your spawn point.",
  "Press F3 to see your coordinates and game information.",
  "Torches prevent hostile mobs from spawning nearby.",
  "Crouch while placing blocks to avoid falling off edges.",
  "Right-click with an empty hand to interact with objects.",
  "Keep your food bar full to regenerate health.",
  "Diamond tools are much more durable than iron ones.",
  "Place water before jumping from great heights to survive.",
  "Enchanting tables require bookshelves to unlock higher levels.",
  "The Nether can be accessed through an obsidian portal.",
  "Always carry a water bucket for emergencies.",
  "Craft a shield to block incoming attacks.",
  "Villagers can trade valuable items for emeralds.",
  "Use a map to keep track of your exploration.",
  "Place a chest on a donkey to increase your inventory space.",
];

export function getRandomTip(): string {
  return tips[Math.floor(Math.random() * tips.length)];
}
