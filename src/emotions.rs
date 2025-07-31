use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionTag {
    pub id: &'static str,
    pub emoji: &'static str,
    pub name: &'static str,
    pub name_en: &'static str,
}

pub const EMOTION_TAGS: [EmotionTag; 21] = [
    EmotionTag {
        id: "happy",
        emoji: "😊",
        name: "행복",
        name_en: "Happy",
    },
    EmotionTag {
        id: "sad",
        emoji: "😢",
        name: "슬픔",
        name_en: "Sad",
    },
    EmotionTag {
        id: "angry",
        emoji: "😡",
        name: "분노",
        name_en: "Angry",
    },
    EmotionTag {
        id: "fear",
        emoji: "😨",
        name: "두려움",
        name_en: "Fear",
    },
    EmotionTag {
        id: "surprise",
        emoji: "😮",
        name: "놀람",
        name_en: "Surprise",
    },
    EmotionTag {
        id: "peaceful",
        emoji: "😌",
        name: "평온",
        name_en: "Peaceful",
    },
    EmotionTag {
        id: "love",
        emoji: "💕",
        name: "사랑",
        name_en: "Love",
    },
    EmotionTag {
        id: "celebration",
        emoji: "🎉",
        name: "축하",
        name_en: "Celebration",
    },
    EmotionTag {
        id: "achievement",
        emoji: "💪",
        name: "성취감",
        name_en: "Achievement",
    },
    EmotionTag {
        id: "inspiration",
        emoji: "🎨",
        name: "영감",
        name_en: "Inspiration",
    },
    EmotionTag {
        id: "delicious",
        emoji: "🍜",
        name: "맛있음",
        name_en: "Delicious",
    },
    EmotionTag {
        id: "music",
        emoji: "🎵",
        name: "음악",
        name_en: "Music",
    },
    EmotionTag {
        id: "beauty",
        emoji: "🌸",
        name: "아름다움",
        name_en: "Beauty",
    },
    EmotionTag {
        id: "memory",
        emoji: "💭",
        name: "추억",
        name_en: "Memory",
    },
    EmotionTag {
        id: "energy",
        emoji: "🏃‍♂️",
        name: "활력",
        name_en: "Energy",
    },
    EmotionTag {
        id: "tired",
        emoji: "😴",
        name: "피곤함",
        name_en: "Tired",
    },
    EmotionTag {
        id: "lonely",
        emoji: "🪞",
        name: "외로움",
        name_en: "Lonely",
    },
    EmotionTag {
        id: "nostalgic",
        emoji: "📷",
        name: "그리움",
        name_en: "Nostalgic",
    },
    EmotionTag {
        id: "anxious",
        emoji: "😬",
        name: "불안함",
        name_en: "Anxious",
    },
    EmotionTag {
        id: "grateful",
        emoji: "🙏",
        name: "감사함",
        name_en: "Grateful",
    },
    EmotionTag {
        id: "hopeful",
        emoji: "🌤️",
        name: "희망",
        name_en: "Hopeful",
    },
];

pub fn get_emotion_by_id(id: &str) -> Option<&'static EmotionTag> {
    EMOTION_TAGS.iter().find(|emotion| emotion.id == id)
}

pub fn get_all_emotions() -> &'static [EmotionTag] {
    &EMOTION_TAGS
}

pub fn is_valid_emotion_id(id: &str) -> bool {
    EMOTION_TAGS.iter().any(|emotion| emotion.id == id)
} 