// #![allow(dead_code, unused_variables)]
#![allow(dead_code)]

// 多线程不安全：程序逻辑-下一个空块必定是要分配的簇，但实际上会在写入之前预先检查下一个空簇，然后在写入时再次检查下一个空簇。单线程下两个结果必定相同，多线程下不一定。
// TODO：预先分配首簇，然后根据首簇写数据。

fn main() {
    let mut virtual_disk: DiskManager;
    let mut buf_str = String::new();

    // 是否从磁盘中读取vd文件初始化
    loop {
        pinfo();
        print!("Do you want to try to load file-sys.vd? [Y/N] ");
        stdout().flush().unwrap();
        stdin().read_line(&mut buf_str).unwrap();
        let first_char = buf_str.as_str().trim().chars().next().unwrap();

        virtual_disk = match first_char {
            'N' | 'n' => {
                pinfo();
                println!("Will not load vd file from disk.\n");

                // 默认根目录配置
                let root_dir_config: Directory = Directory {
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
                };

                DiskManager::new(root_dir_config)
            }
            'Y' | 'y' => {
                pinfo();
                println!("Trying to load vd file from disk...\n");
                let data = fs::read("./file-sys.vd").unwrap();

                bincode::deserialize(data.as_slice()).unwrap()
            }
            _ => {
                println!("\nIncorrect input.");
                continue;
            }
        };

        break;
    }

    // 交互界面
    println!("{}", UI_HELP);

    loop {
        // 清空buffer
        buf_str.clear();
        print!("> ");
        stdout().flush().unwrap();
        stdin().read_line(&mut buf_str).unwrap();
        // 去除首尾空格
        let command_line = String::from(buf_str.trim());

        // 分支-test
        if let Some(cl) = command_line.strip_prefix("test ") {
            // 分支-create
            if let Some(cl) = cl.strip_prefix("create") {
                let data = format!("File has been created at {:?} .", SystemTime::now());
                let cl_trim = cl.trim();
                let name = if cl_trim.is_empty() {
                    // 没有输入名字
                    format!("test-{}", (rand::random::<f32>() * 100_f32) as usize)
                } else {
                    // 输入了名字
                    cl_trim.to_string()
                };
                virtual_disk.create_file_with_data(name.as_str(), data.as_bytes());
            }
        } else if command_line.starts_with("help") {
            // 显示菜单
            println!("{}", UI_HELP);
        } else if command_line.starts_with("exit") {
            // 跳出循环，结束程序
            pinfo();
            println!("Exiting system...\n");
            break;
        } else if command_line.starts_with("save") {
            // 保存系统
            pinfo();
            println!("Saving...");
            let data = bincode::serialize(&virtual_disk).unwrap();
            fs::write(SAVE_FILE_NAME, data.as_slice()).unwrap();
            pinfo();
            println!("The virtual disk system has been saved.\n");
        } else if command_line.starts_with("ls") {
            // 列出目录文件
            println!("{}", virtual_disk.cur_dir);
        } else if let Some(name) = command_line.strip_prefix("cd ") {
            // 切换到当前目录的某个文件夹
            pinfo();
            println!("Set Location to: {} ...", name);
            virtual_disk.set_current_directory(name);
        } else if let Some(command_line) = command_line.strip_prefix("cat ") {
            // 显示文件内容
            let name = command_line.trim();
            let data = virtual_disk.read_file_by_name(name);
            println!("{}", str::from_utf8(data.as_slice()).unwrap());
        } else if let Some(command_line) = command_line.strip_prefix("mkdir ") {
            // 创建新文件夹
            let name = command_line.trim();
            virtual_disk.new_directory_to_disk(name).unwrap();
        } else if command_line.starts_with("diskinfo") {
            // 返回磁盘信息
            let (disk_size, num_used, num_not_used) = virtual_disk.get_disk_info();
            println!(
                "Disk sized {} Bytes, {} Bytes used, {} Bytes available.",
                disk_size,
                num_used * BLOCK_SIZE,
                num_not_used * BLOCK_SIZE
            );
        } else if let Some(command_line) = command_line.strip_prefix("rm ") {
            let name = command_line.trim();
            virtual_disk
                .delete_file_by_name(name)
                .expect("[ERROR]\tDELETE FILE FAILED!");
        } else {
            println!("Unknown Command.");
        }
    }

    // virtual_disk.new_directory_to_disk("test").unwrap();
    // virtual_disk.set_current_directory("test");
    // println!(2
    //     "[DEBUG]\tCurrent Dir: {:?}\n\tSwitching to parrent dir...",
    //     virtual_disk.cur_dir.name
    // );
    // // virtual_disk.set_current_directory("..");
    // // pdebug(); println!("Current Dir: {:?}", virtual_disk.cur_dir.name);
    // pdebug(); println!("Trying to create file...");
    // virtual_disk.create_file_with_data("helloworld.txt", String::from("Hello World!").as_bytes());
    // pdebug(); println!("File list: {:?}", virtual_disk.cur_dir.files);
    // println!(
    //     "[DEBUG]\tFile content: {:?}",
    //     String::from_utf8(virtual_disk.read_file_by_name("helloworld.txt")).unwrap()
    // );
}

use ansi_rgb::Foreground;
use core::panic;
use std::{str, time::SystemTime};
// use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{stdin, stdout, Write},
    mem::size_of,
    str::FromStr,
    string::String,
    usize,
    vec::Vec,
};

const BLOCK_SIZE: usize = 1024; // 簇大小：1KiB
const BLOCK_COUNT: usize = 1000; // 簇数量
const EOF_BYTE: u8 = 254; // 定义从后向前扫描时的EoF
const SAVE_FILE_NAME: &str = "file-sys.vd"; // 默认保存的文件名
const UI_HELP: &str = "\
\n==================================================\
\n           IvanD's Basic File System\
\n==================================================\
\nHelp:\
\n\tcd <dirname>: Change current dir.\
\n\tmkdir <dir name>: Create a new dir.\
\n\tls : List all files and dir in current dir.\
\n\tcat <filename>: Show the file content.\
\n\trm <filename>: Delete a file on disk.\
\n\tdiskinfo : Show some info about disk.\
\n\tsave : Save this virtual disk to file 'file-sys.vd'\
\n\texit : Exit the system. 
\n\
\nTesting:\
\n\ttest create: Create a random file to test.\
\n\
\nSystem Inner Function:\
\n\tfn create_file_with_data(&mut self, name: &str, data: &[u8])\
\n\tfn rename_file(&mut self, old: &str, new: &str)\
\n\tfn delete_file_by_name(&mut self, name: &str)\
\n\tfn delete_file_by_name(&mut self, name: &str)\
\n\tfn read_file_by_name(&self, name: &str) -> Vec<u8>\
\n"; // UI主菜单

fn pinfo() {
    print!("{}", "[INFO]\t".fg(ansi_rgb::cyan_blue()));
}
fn pdebug() {
    print!("{}", "[DEBUG]\t".fg(ansi_rgb::magenta()));
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum FatItem {
    NotUsed,          // 未使用的簇
    ClusterNo(usize), // 指向下一个的簇号
    BadCluster,       // 坏簇
    EoF,              // 文件结束
}

#[derive(Serialize, Deserialize)]
struct Disk {
    fat: Vec<FatItem>,
    data: Vec<u8>,
}
impl Disk {
    fn new() -> Disk {
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
    fn insert_data_by_offset(&mut self, data: &[u8], offset: usize) {
        self.data
            .splice(offset..(offset + data.len()), data.iter().cloned());
    }
    /// 向disk中的data插入数据。插入数据将覆写相应的位置。
    fn insert_data_by_cluster(&mut self, data: &[u8], cluster: usize) {
        self.insert_data_by_offset(data, cluster * BLOCK_SIZE);
    }

    /// 向disk中的data插入数据。插入数据将覆写相应的位置。
    fn write_data_by_clusters_with_eof(
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
                if insert_eof {
                    // 如果插入EoF，就新建一个buffer，在其中加入EoF后写入disk
                    let mut buffer = (&data[i * BLOCK_SIZE..data.len()]).to_vec();
                    buffer.push(EOF_BYTE);
                    self.insert_data_by_cluster(buffer.as_slice(), clusters[i])
                } else {
                    // 若不插入EoF，就直接写入disk
                    self.insert_data_by_cluster(&data[i * BLOCK_SIZE..data.len()], clusters[i])
                }
            }
        }
    }

    /// 从disk中读取数据。
    fn read_data_by_cluster(&self, cluster: usize) -> Vec<u8> {
        (&self.data[cluster * BLOCK_SIZE..(cluster + 1) * BLOCK_SIZE]).to_vec()
    }
}

#[derive(Serialize, Deserialize)]
struct DiskManager {
    disk: Disk,
    cur_dir: Directory,
}
impl DiskManager {
    /// 初始化新磁盘，返回DiskManager对象
    fn new(root_dir: Directory) -> DiskManager {
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
            cur_dir: root_dir,
        }
    }

    /// 返回一个状态是NotUsed的簇块号
    fn find_next_empty_fat(&self) -> Option<usize> {
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
    fn allocate_free_space_on_fat(
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

    /// 查找一个簇在FAT中所关联的所有文件块
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
    /// 返回：（是否需要插入EoF，需要的总簇数）
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
    fn write_data_to_disk(&mut self, data: &[u8]) -> usize {
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
    fn new_directory_to_disk(&mut self, name: &str) -> Result<(), &'static str> {
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
            name: String::from_str(name).unwrap(),
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
        let mut data: Vec<u8> = Vec::new();
        let clusters = self.get_file_clusters(first_cluster).unwrap();

        // 然后循环读出所有数据
        for cluster in clusters {
            let mut buffer = self.disk.read_data_by_cluster(cluster);
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
    fn create_file_with_data(&mut self, name: &str, data: &[u8]) {
        pinfo();
        println!("Creating new file in current dir...");
        // 写入数据
        let first_cluster = self.write_data_to_disk(data);
        // 创建新FCB并插入当前目录中
        let fcb = Fcb {
            name: String::from_str(name).unwrap(),
            file_type: FileType::File,
            first_cluster,
            length: data.len(),
        };
        self.cur_dir.files.push(fcb);
    }

    /// 通过文件名读取文件
    fn read_file_by_name(&self, name: &str) -> Vec<u8> {
        let (_index, fcb) = self.cur_dir.get_fcb_by_name(name).unwrap();
        self.get_file_by_fcb(fcb)
    }

    /// 通过文件名删除文件
    fn delete_file_by_name(&mut self, name: &str) -> Result<(), String> {
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
    fn set_current_directory(&mut self, name: &str) {
        // 保存当前文件夹
        let dir_cloned = self.cur_dir.clone();
        self.save_directory_to_disk(&dir_cloned);
        // 通过名字获取下一个文件夹
        let (_index, dir_fcb) = self.cur_dir.get_fcb_by_name(name).unwrap();

        let dir = self.get_directory_by_fcb(dir_fcb);
        self.cur_dir = dir;
    }

    /// 保存文件夹到磁盘，返回第一个簇号——更改被保存，原目录文件将在磁盘上被覆盖
    fn save_directory_to_disk(&mut self, dir: &Directory) -> usize {
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
    fn rename_file_by_name(&mut self, old: &str, new: &str) {
        let (index, fcb) = self.cur_dir.get_fcb_by_name(old).unwrap();
        let new_fcb = Fcb {
            name: String::from(new),
            ..fcb.to_owned()
        };
        self.cur_dir.files[index] = new_fcb;
    }

    /// 获取部分磁盘信息
    /// 返回 磁盘总大小/Byte，已分配簇数量、未分配簇的数量
    fn get_disk_info(&self) -> (usize, usize, usize) {
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
    fn move_fcb_between_dirs_by_name(&mut self, name: &str, des_dir: &mut Directory) {
        let fcb = self
            .cur_dir
            .files
            .remove(self.cur_dir.get_index_by_name(name).unwrap());
        des_dir.files.push(fcb);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum FileType {
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
struct Fcb {
    name: String,         // 文件名
    file_type: FileType,  // 文件类型
    first_cluster: usize, // 起始块号
    length: usize,        // 文件大小
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Directory {
    name: String,
    files: Vec<Fcb>,
}
impl Directory {
    fn new(name: &str) -> Directory {
        Directory {
            name: String::from_str(name).unwrap(),
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
