use std::mem::size_of;

use serde::{Deserialize, Serialize};

/// 簇大小：1KiB
pub const BLOCK_SIZE: usize = 1024;
/// 簇数量
pub const BLOCK_COUNT: usize = 1000;
/// 定义从后向前扫描时的EoF
pub const EOF_BYTE: u8 = 255;

#[derive(Serialize, Deserialize)]
pub struct Disk {
    pub fat: Vec<FatItem>,
    data: Vec<u8>,
}
impl Disk {
    pub fn new() -> Disk {
        Disk {
            // 创建FAT文件分配表
            fat: vec![FatItem::NotUsed; BLOCK_COUNT],
            // 数据区，初始值为0，块大小为1024.
            // 每一个块都有一个对应的FAT项，所以真实的数据区域需要在总数中减去FAT项的数据大小
            data: vec![
                0u8;
                (BLOCK_COUNT - size_of::<FatItem>() * BLOCK_COUNT / BLOCK_SIZE - 1)
                    * BLOCK_SIZE
            ],
        }
    }

    /// 向disk的data中插入数据。插入的数据将覆写相应位置的数据。
    pub fn insert_data_by_offset(&mut self, data: &[u8], offset: usize) {
        self.data
            .splice(offset..(offset + data.len()), data.iter().cloned());
    }
    /// 向disk中的data插入数据。插入数据将覆写相应的位置。
    pub fn insert_data_by_cluster(&mut self, data: &[u8], cluster: usize) {
        self.insert_data_by_offset(data, cluster * BLOCK_SIZE);
    }

    /// 向disk中的data插入数据。插入数据将覆写相应的位置。
    pub fn write_data_by_clusters_with_eof(
        &mut self,
        data: &[u8],
        clusters: &[usize],
        insert_eof: bool,
    ) {
        for i in 0..clusters.len() {
            if i < clusters.len() - 1 {
                // 正常分BLOCK_SIZE写入簇
                self.insert_data_by_cluster(
                    &data[i * BLOCK_SIZE..(i + 1) * BLOCK_SIZE],
                    clusters[i],
                );
            } else {
                // 开始写入最后一个块
                let mut buffer: Vec<u8> = Vec::with_capacity(BLOCK_SIZE);
                buffer.extend((&data[i * BLOCK_SIZE..data.len()]).iter());
                if insert_eof {
                    // 插入EoF
                    buffer.push(EOF_BYTE);
                }
                if buffer.len() < BLOCK_SIZE {
                    // 若未到 BLOCK_SIZE 则用0填充
                    let mut zero = vec![0u8; BLOCK_SIZE - buffer.len()];
                    buffer.append(&mut zero);
                }
                self.insert_data_by_cluster(buffer.as_slice(), clusters[i])
            }
        }
    }

    /// 从disk中读取数据。
    pub fn read_data_by_cluster(&self, cluster: usize) -> Vec<u8> {
        (&self.data[cluster * BLOCK_SIZE..(cluster + 1) * BLOCK_SIZE]).to_vec()
    }

    /// 工具给出的簇号，读出所有数据，并且检测EoF。
    pub fn read_data_by_clusters_without_eof(&self, clusters: &[usize]) -> Vec<u8> {
        let mut data: Vec<u8> = Vec::with_capacity(clusters.len() * BLOCK_SIZE);

        // 循环读出所有数据
        for cluster in clusters {
            let mut buffer = self.read_data_by_cluster(*cluster);
            data.append(&mut buffer);
        }
        // 从后向前查找，从EoF开始截断。若未找到EoF则直接返回。
        for i in 1..BLOCK_SIZE {
            let index = data.len() - i;
            if data[index] == EOF_BYTE {
                // 不加不减，刚好将EoF截断在外
                data.truncate(index);
                break;
            }
        }

        data
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FatItem {
    NotUsed,          // 未使用的簇
    ClusterNo(usize), // 指向下一个的簇号
    BadCluster,       // 坏簇
    EoF,              // 文件结束
}
