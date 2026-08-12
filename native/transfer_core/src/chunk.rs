use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_CHUNK_SIZE: u32 = 4 * 1024 * 1024;
pub const MAX_BUFFER_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_DATA_CHANNELS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSpec {
    pub index: u32,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkPlan {
    file_size: u64,
    chunk_size: u32,
    chunk_count: u32,
}

impl ChunkPlan {
    pub fn new(file_size: u64, chunk_size: u32) -> Result<Self, ChunkError> {
        if chunk_size == 0 {
            return Err(ChunkError::ZeroChunkSize);
        }

        let count = file_size.div_ceil(u64::from(chunk_size));
        let chunk_count = u32::try_from(count).map_err(|_| ChunkError::TooManyChunks(count))?;

        Ok(Self {
            file_size,
            chunk_size,
            chunk_count,
        })
    }

    pub fn chunk_count(&self) -> u32 {
        self.chunk_count
    }

    pub fn chunk(&self, index: u32) -> Result<ChunkSpec, ChunkError> {
        if index >= self.chunk_count {
            return Err(ChunkError::IndexOutOfRange {
                index,
                count: self.chunk_count,
            });
        }

        let offset = u64::from(index) * u64::from(self.chunk_size);
        let remaining = self.file_size - offset;
        let length = remaining.min(u64::from(self.chunk_size)) as u32;
        Ok(ChunkSpec {
            index,
            offset,
            length,
        })
    }

    pub fn missing_chunks(&self, resume: &ResumeMap) -> Result<Vec<ChunkSpec>, ChunkError> {
        if resume.chunk_count != self.chunk_count {
            return Err(ChunkError::ResumeMapMismatch {
                expected: self.chunk_count,
                actual: resume.chunk_count,
            });
        }

        (0..self.chunk_count)
            .filter(|index| !resume.contains(*index))
            .map(|index| self.chunk(index))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeMap {
    chunk_count: u32,
    completed_count: u32,
    words: Vec<u64>,
}

impl ResumeMap {
    pub fn new(chunk_count: u32) -> Self {
        let word_count = usize::try_from(chunk_count.div_ceil(64)).unwrap_or(usize::MAX);
        Self {
            chunk_count,
            completed_count: 0,
            words: vec![0; word_count],
        }
    }

    pub fn mark_complete(&mut self, index: u32) -> Result<bool, ChunkError> {
        if index >= self.chunk_count {
            return Err(ChunkError::IndexOutOfRange {
                index,
                count: self.chunk_count,
            });
        }

        let word = usize::try_from(index / 64).expect("u32 always fits usize");
        let mask = 1_u64 << (index % 64);
        if self.words[word] & mask != 0 {
            return Ok(false);
        }

        self.words[word] |= mask;
        self.completed_count += 1;
        Ok(true)
    }

    pub fn contains(&self, index: u32) -> bool {
        if index >= self.chunk_count {
            return false;
        }
        let word = usize::try_from(index / 64).expect("u32 always fits usize");
        self.words[word] & (1_u64 << (index % 64)) != 0
    }

    pub fn is_complete(&self) -> bool {
        self.completed_count == self.chunk_count
    }

    pub fn to_bitmap_bytes(&self) -> Vec<u8> {
        let byte_count = usize::try_from(self.chunk_count.div_ceil(8)).unwrap_or(usize::MAX);
        let mut bytes = vec![0_u8; byte_count];
        for index in 0..self.chunk_count {
            if self.contains(index) {
                bytes[index as usize / 8] |= 1_u8 << (index % 8);
            }
        }
        bytes
    }

    pub fn from_bitmap_bytes(chunk_count: u32, bytes: &[u8]) -> Result<Self, ChunkError> {
        let expected = usize::try_from(chunk_count.div_ceil(8)).unwrap_or(usize::MAX);
        if bytes.len() != expected {
            return Err(ChunkError::InvalidBitmapLength {
                expected,
                actual: bytes.len(),
            });
        }
        let mut map = Self::new(chunk_count);
        for index in 0..chunk_count {
            if bytes[index as usize / 8] & (1_u8 << (index % 8)) != 0 {
                map.mark_complete(index)?;
            }
        }
        Ok(map)
    }

    pub fn completed_bytes(&self, plan: &ChunkPlan) -> Result<u64, ChunkError> {
        if self.chunk_count != plan.chunk_count {
            return Err(ChunkError::ResumeMapMismatch {
                expected: plan.chunk_count,
                actual: self.chunk_count,
            });
        }

        let mut bytes = 0_u64;
        for index in 0..self.chunk_count {
            if self.contains(index) {
                bytes += u64::from(plan.chunk(index)?.length);
            }
        }
        Ok(bytes)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChunkError {
    #[error("数据块大小不能为零")]
    ZeroChunkSize,
    #[error("文件需要的数据块数量过多: {0}")]
    TooManyChunks(u64),
    #[error("数据块索引 {index} 超出范围，总数为 {count}")]
    IndexOutOfRange { index: u32, count: u32 },
    #[error("断点信息与文件不匹配，期望 {expected} 块，实际 {actual} 块")]
    ResumeMapMismatch { expected: u32, actual: u32 },
    #[error("断点位图长度无效，期望 {expected} 字节，实际 {actual} 字节")]
    InvalidBitmapLength { expected: usize, actual: usize },
}

#[cfg(test)]
mod tests {
    use super::{ChunkPlan, ResumeMap};

    #[test]
    fn resume_plan_returns_only_missing_chunks() {
        let plan = ChunkPlan::new(10, 4).expect("valid plan");
        let mut resume = ResumeMap::new(plan.chunk_count());
        resume.mark_complete(0).expect("first chunk");
        resume.mark_complete(2).expect("last chunk");

        let missing = plan.missing_chunks(&resume).expect("matching resume map");

        assert_eq!(missing, vec![plan.chunk(1).expect("middle chunk")]);
        assert_eq!(resume.completed_bytes(&plan).expect("byte count"), 6);
        assert!(!resume.is_complete());
        assert_eq!(
            ResumeMap::from_bitmap_bytes(plan.chunk_count(), &resume.to_bitmap_bytes())
                .expect("bitmap round trip"),
            resume
        );
    }

    #[test]
    fn mark_complete_is_idempotent_and_never_inflates_counts() {
        let plan = ChunkPlan::new(10, 4).expect("valid plan");
        let mut resume = ResumeMap::new(plan.chunk_count());

        assert!(resume.mark_complete(1).expect("first mark"));
        assert!(
            !resume.mark_complete(1).expect("duplicate mark is ignored"),
            "重复标记同一块必须返回 false"
        );
        assert_eq!(
            resume.completed_bytes(&plan).expect("byte count"),
            4,
            "重复块不能重复累计字节数"
        );
        assert_eq!(
            resume.to_bitmap_bytes(),
            vec![0b0000_0010],
            "位图中同一块只占一位"
        );
        assert!(!resume.mark_complete(1).expect("still duplicate"));
        assert!(resume.mark_complete(2).expect("new chunk"));
        assert_eq!(
            resume.completed_bytes(&plan).expect("byte count"),
            6,
            "chunk 1 为 4 字节，chunk 2 为 2 字节"
        );
    }
}
