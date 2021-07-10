pub mod disk;
use disk::{Disk, FatItem, BLOCK_COUNT, BLOCK_SIZE};

use ansi_rgb::Foreground;
use core::panic;
use serde::{Deserialize, Serialize};
use std::str;
use std::{fmt, string::String, usize, vec::Vec};

pub fn pinfo() {
    print!("{}", "[INFO]\t".fg(ansi_rgb::cyan_blue()));
}
pub fn pdebug() {
    print!("{}", "[DEBUG]\t".fg(ansi_rgb::magenta()));
}

#[derive(Serialize, Deserialize)]
pub struct DiskManager {
    pub disk: Disk,
    pub cur_dir: Directory,
}
impl DiskManager {
    /// 初始化新磁盘，返回DiskManager对象。若输入None，则自动创建默认配置。
    pub fn new(root_dir: Option<Directory>) -> DiskManager {
        pinfo();
        println!("Creating new disk...");
        // 生成虚拟磁盘
        let mut disk = Disk::new();
        {
            // 放置第一个根目录
            let dir_data = bincode::serialize(&root_dir).unwrap();
            disk.insert_data_by_offset(dir_data.as_slice(), 0);
        }
        disk.fat[0] = FatItem::EoF;

        DiskManager {
            disk,
            cur_dir: match root_dir {
                // 默认根目录配置
                None => Directory {
                    name: String::from("root"),
                    files: vec![
                        Fcb {
                            name: String::from(".."),
                            file_type: FileType::Directory,
                            first_cluster: 0,
                            length: 0,
                        },
                        Fcb {
                            name: String::from("."),
                            file_type: FileType::Directory,
                            first_cluster: 0,
                            length: 0,
                        },
                    ],
                },
                Some(dir) => dir,
            },
        }
    }

    /// 返回一个状态是NotUsed的簇块号
    pub fn find_next_empty_fat(&self) -> Option<usize> {
        let mut res = None;
        for i in 0..(self.disk.fat.len() - 1) {
            if let FatItem::NotUsed = self.disk.fat[i] {
                res = Some(i);
                break;
            }
        }

        res
    }

    /// 输入需要分配的簇数量，在FAT表上标记为已用（分配新空间），返回被分配的簇号数组。
    pub fn allocate_free_space_on_fat(
        &mut self,
        clusters_needed: usize,
    ) -> Result<Vec<usize>, &'static str> {
        pinfo();
        println!("Allocating new space...");

        let mut clusters: Vec<usize> = Vec::with_capacity(clusters_needed);
        for i in 0..clusters_needed {
            // 找到新未用的簇
            clusters.push(match self.find_next_empty_fat() {
                Some(cluster) => cluster,
                _ => return Err("[ERROR]\tCannot find a NotUsed FatItem!"),
            });
            // this_cluster：每次循环进行操作的cluster
            let this_cluster = clusters[i];

            // 对磁盘写入数据
            pdebug();
            println!("Found new empty cluster: {}", this_cluster);
            if i != 0 {
                // 中间的和最后一次的写入
                // 将上一块改写成指向当前块的FatItem
                self.disk.fat[clusters[i - 1]] = FatItem::ClusterNo(this_cluster);
            }
            // 默认当前块是最后的
            self.disk.fat[this_cluster] = FatItem::EoF;
        }

        Ok(clusters)
    }

    /// 查找以`first_cluster`为开头的在FAT中所关联的所有文件块。
    ///
    /// # 错误
    ///
    /// 当检测到簇指向一个未使用的簇的时候，返回那个被指向的未使用的簇的索引。

    fn get_file_clusters(&self, first_cluster: usize) -> Result<Vec<usize>, String> {
        pinfo();
        println!("Searching file clusters...");
        let mut clusters: Vec<usize> = Vec::new();
        let mut this_cluster = first_cluster;

        // 第一个簇
        clusters.push(first_cluster);

        // 然后循环读出之后所有簇
        loop {
            match self.disk.fat[this_cluster] {
                FatItem::ClusterNo(cluster) => {
                    pdebug();
                    println!("Found next cluster: {}.", cluster);
                    clusters.push(cluster);
                    this_cluster = cluster;
                }
                FatItem::EoF => {
                    pdebug();
                    println!("Found EoF cluster: {}.", this_cluster);
                    break Ok(clusters);
                }
                FatItem::BadCluster => {
                    // 跳过坏簇
                    this_cluster += 1;
                    continue;
                }
                _ => {
                    break Err(format!(
                        "[ERROR]\tBad cluster detected at {}!",
                        this_cluster
                    ))
                }
            }
        }
    }

    /// 删除已经被分配的簇（置空），返回已经被删除的簇号数组。
    fn delete_space_on_fat(&mut self, first_cluster: usize) -> Result<Vec<usize>, String> {
        pinfo();
        println!("Deleting Fat space...");
        let clusters_result = self.get_file_clusters(first_cluster);
        let clusters = clusters_result.clone().unwrap();
        for cluster in clusters {
            self.disk.fat[cluster] = FatItem::NotUsed;
        }

        clusters_result
    }

    /// 重新分配已经被分配的簇，按需要的簇数量分配，原簇将被置空。
    fn reallocate_free_space_on_fat(
        &mut self,
        first_cluster: usize,
        clusters_needed: usize,
    ) -> Vec<usize> {
        pinfo();
        println!("Realocating Fat space...");
        // 删除原先的簇
        self.delete_space_on_fat(first_cluster).unwrap();
        // 分配新的簇 - 多线程下第一簇可能不同，多线程不安全
        self.allocate_free_space_on_fat(clusters_needed).unwrap()
    }

    /// 计算写入文件需要的簇数量——针对EoF
    /// 返回（`bool`: 是否需要插入EoF，`usize`: 需要的总簇数）
    fn calc_clusters_needed_with_eof(length: usize) -> (bool, usize) {
        // 判断需要写入的总簇数
        let mut clusters_needed: f32 = length as f32 / BLOCK_COUNT as f32;
        // 判断cluster是否是整数。如果是，就不写入结束标志。
        let insert_eof = if (clusters_needed - clusters_needed as usize as f32) < 0.0000000001 {
            false
        } else {
            clusters_needed = clusters_needed.ceil();
            true
        };
        let clusters_needed: usize = clusters_needed as usize;

        (insert_eof, clusters_needed)
    }

    /// 提供想要写入的数据，返回数据的开始簇块号，可在FAT中查找
    pub fn write_data_to_disk(&mut self, data: &[u8]) -> usize {
        pinfo();
        println!("Writing data to disk...");

        let (insert_eof, clusters_needed) = DiskManager::calc_clusters_needed_with_eof(data.len());

        let clusters = self.allocate_free_space_on_fat(clusters_needed).unwrap();

        self.disk
            .write_data_by_clusters_with_eof(data, clusters.as_slice(), insert_eof);

        pdebug();
        println!("Writing finished. Returned clusters: {:?}", clusters);

        clusters[0]
    }

    /// 提供目录名，在当前目录中新建目录，同时写入磁盘。
    pub fn new_directory_to_disk(&mut self, name: &str) -> Result<(), &'static str> {
        // 新文件夹写入磁盘块
        pinfo();
        println!("Creating dir: {}.", name);
        pdebug();
        println!("Trying to write to disk...");

        if let Some(_fcb) = self.cur_dir.get_fcb_by_name(name) {
            return Err("[ERROR]\tThere's already a directory with a same name!");
        }

        let mut new_directory = Directory::new(name);
        // 加入“..”
        new_directory.files.push(Fcb {
            name: String::from(".."),
            file_type: FileType::Directory,
            first_cluster: self.cur_dir.files[1].first_cluster,
            length: 0,
        });
        // 加入“.”
        // TODO: 多线程不安全
        new_directory.files.push(Fcb {
            name: String::from("."),
            file_type: FileType::Directory,
            first_cluster: self.find_next_empty_fat().unwrap(),
            length: 0,
        });

        let bin_dir = bincode::serialize(&new_directory).unwrap();

        pdebug();
        println!("Dir bytes: {:?}", bin_dir);
        let first_block = self.write_data_to_disk(&bin_dir);

        pdebug();
        println!("Trying to add dir to current dir...");
        // 在文件夹中添加新文件夹
        self.cur_dir.files.push(Fcb {
            name: String::from(name),
            file_type: FileType::Directory,
            first_cluster: first_block,
            length: 0,
        });
        pdebug();
        println!("Created dir {}.", name);

        Ok(())
    }

    /// 提供簇号，读出所有数据。
    fn get_data_by_first_cluster(&self, first_cluster: usize) -> Vec<u8> {
        pdebug();
        println!("Getting data from disk by clusters...");

        let clusters = self.get_file_clusters(first_cluster).unwrap();
        let data = self
            .disk
            .read_data_by_clusters_without_eof(clusters.as_slice());

        pdebug();
        println!("Data read: {:?}", &data);

        data
    }

    /// 通过FCB块找到目录项
    fn get_directory_by_fcb(&self, dir_fcb: &Fcb) -> Directory {
        pinfo();
        println!("Getting dir by FCB...\n\tFCB: {:?}", dir_fcb);
        match dir_fcb.file_type {
            FileType::Directory => {
                let data_dir = self.get_data_by_first_cluster(dir_fcb.first_cluster);
                pdebug();
                println!("Trying to deserialize data read from disk...");
                let dir: Directory = bincode::deserialize(data_dir.as_slice()).unwrap();
                pdebug();
                println!("Getting dir finished.");
                dir
            }
            _ => panic!("[ERROR]\tGet Directory recieved a non-Directory FCB!"),
        }
    }

    /// 通过FCB块找到文件
    fn get_file_by_fcb(&self, fcb: &Fcb) -> Vec<u8> {
        pinfo();
        println!("Getting file data by FCB...\n\tFCB: {:?}", fcb);
        match fcb.file_type {
            FileType::File => self.get_data_by_first_cluster(fcb.first_cluster),
            _ => panic!("[ERROR]\tGet File recieved a non-File FCB!"),
        }
    }

    /// 通过FCB块删除文件
    fn delete_file_by_fcb(&mut self, fcb: &Fcb) -> Result<(), String> {
        self.delete_file_by_fcb_with_index(
            fcb,
            Some(self.cur_dir.get_index_by_name(fcb.name.as_str()).unwrap()),
        )
    }

    /// 通过FCB块删除文件，参数中含有FCB块在dir中的序号。
    fn delete_file_by_fcb_with_index(
        &mut self,
        fcb: &Fcb,
        index: Option<usize>,
    ) -> Result<(), String> {
        if let FileType::Directory = fcb.file_type {
            let dir = self.get_directory_by_fcb(fcb);
            if dir.files.len() > 2 {
                return Err(String::from("[ERROR]\tThe Directory is not empty!"));
            }
        }
        pdebug();
        println!(
            "Trying to set all NotUsed clutster of file '{}' on FAT...",
            fcb.name
        );
        // 直接返回删除文件的结果
        if let Err(err) = self.delete_space_on_fat(fcb.first_cluster) {
            return Err(err);
        }
        // 若给定index非None，则删除目录下的FCB条目
        if let Some(i) = index {
            self.cur_dir.files.remove(i);
        }

        Ok(())
    }

    /// 在当前文件夹创建新文件并写入
    pub fn create_file_with_data(&mut self, name: &str, data: &[u8]) {
        pinfo();
        println!("Creating new file in current dir...");
        // 写入数据
        let first_cluster = self.write_data_to_disk(data);
        // 创建新FCB并插入当前目录中
        let fcb = Fcb {
            name: String::from(name),
            file_type: FileType::File,
            first_cluster,
            length: data.len(),
        };
        self.cur_dir.files.push(fcb);
    }

    /// 通过文件名读取文件
    pub fn read_file_by_name(&self, name: &str) -> Vec<u8> {
        let (_index, fcb) = self.cur_dir.get_fcb_by_name(name).unwrap();
        self.get_file_by_fcb(fcb)
    }

    /// 通过文件名删除文件
    pub fn delete_file_by_name(&mut self, name: &str) -> Result<(), String> {
        let index = self.cur_dir.get_index_by_name(name).unwrap();
        // 从dir中先删除fcb，如果删除失败再还回来
        pdebug();
        println!("Trying to delete file in dir file list...");
        let fcb = self.cur_dir.files.remove(index);
        let res = self.delete_file_by_fcb_with_index(&fcb, None);

        if res.is_err() {
            self.cur_dir.files.push(fcb);
        }

        res
    }

    /// 通过文件夹名设置当前文件夹
    pub fn set_current_directory(&mut self, name: &str) {
        // 保存当前文件夹
        let dir_cloned = self.cur_dir.clone();
        self.save_directory_to_disk(&dir_cloned);
        // 通过名字获取下一个文件夹
        let (_index, dir_fcb) = self.cur_dir.get_fcb_by_name(name).unwrap();

        let dir = self.get_directory_by_fcb(dir_fcb);
        self.cur_dir = dir;
    }

    /// 保存文件夹到磁盘，返回第一个簇号——更改被保存，原目录文件将在磁盘上被覆盖
    pub fn save_directory_to_disk(&mut self, dir: &Directory) -> usize {
        pdebug();
        println!("Trying to saving dir...");
        let data = bincode::serialize(dir).unwrap();
        let (insert_eof, clusters_needed) = DiskManager::calc_clusters_needed_with_eof(data.len());
        let reallocated_clusters =
            self.reallocate_free_space_on_fat(self.cur_dir.files[1].first_cluster, clusters_needed);
        self.disk.write_data_by_clusters_with_eof(
            data.as_slice(),
            reallocated_clusters.as_slice(),
            insert_eof,
        );

        reallocated_clusters[0]
    }

    /// 文件改名，没啥好说的。
    pub fn rename_file_by_name(&mut self, old: &str, new: &str) {
        let (index, fcb) = self.cur_dir.get_fcb_by_name(old).unwrap();
        let new_fcb = Fcb {
            name: String::from(new),
            ..fcb.to_owned()
        };
        self.cur_dir.files[index] = new_fcb;
    }

    /// 获取部分磁盘信息
    /// 返回 磁盘总大小/Byte，已分配簇数量、未分配簇的数量
    pub fn get_disk_info(&self) -> (usize, usize, usize) {
        let disk_size = BLOCK_SIZE * BLOCK_COUNT;
        let mut num_used = 0usize;
        let mut num_not_used = 0usize;

        for fat_item in &self.disk.fat {
            match fat_item {
                FatItem::ClusterNo(_no) => num_used += 1,
                FatItem::EoF => num_used += 1,
                FatItem::NotUsed => num_not_used += 1,
                _ => (),
            }
        }

        (disk_size, num_used, num_not_used)
    }

    /// FCB的移动
    pub fn move_fcb_between_dirs_by_name(&mut self, name: &str, des_dir: &mut Directory) {
        let fcb = self
            .cur_dir
            .files
            .remove(self.cur_dir.get_index_by_name(name).unwrap());
        des_dir.files.push(fcb);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FileType {
    File,
    Directory,
}
impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileType::Directory => write!(f, "Directory"),
            FileType::File => write!(f, "File"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Fcb {
    name: String,         // 文件名
    file_type: FileType,  // 文件类型
    first_cluster: usize, // 起始块号
    length: usize,        // 文件大小
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Directory {
    name: String,
    files: Vec<Fcb>,
}
impl Directory {
    fn new(name: &str) -> Directory {
        Directory {
            name: String::from(name),
            files: Vec::with_capacity(2),
        }
    }

    /// 通过文件名获取文件在files中的索引和文件FCB
    fn get_fcb_by_name(&self, name: &str) -> Option<(usize, &Fcb)> {
        let mut res = None;
        for i in 0..self.files.len() {
            if self.files[i].name.as_str() == name {
                res = Some((i, &self.files[i]));
                break;
            }
        }

        res
    }

    /// 通过文件名获取文件在files中的索引和文件FCB
    fn get_index_by_name(&self, name: &str) -> Option<usize> {
        let mut res = None;
        for i in 0..self.files.len() {
            if self.files[i].name.as_str() == name {
                res = Some(i);
                break;
            }
        }

        res
    }
}
impl fmt::Display for Directory {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // 仅将 self 的第一个元素写入到给定的输出流 `f`。返回 `fmt:Result`，此
        // 结果表明操作成功或失败。注意 `write!` 的用法和 `println!` 很相似。
        writeln!(f, "Directroy '{}' Files:", self.name)?;
        for file in &self.files {
            writeln!(
                f,
                "{}\t\t{}\t\tLength: {}",
                file.name, file.file_type, file.length
            )?;
        }

        fmt::Result::Ok(())
    }
}
