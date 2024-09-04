cmake_minimum_required(VERSION 3.15)
project(raylib-template)

execute_process(COMMAND git submodule init
                COMMAND git submodule update --depth=1)

set(BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(BUILD_GAMES OFF CACHE BOOL "" FORCE)

add_subdirectory(third-party/raylib)

file (GLOB_RECURSE LOCAL_SOURCES "./src/*.c*")
set (LOCAL_INCLUDE 
    "third-party/raylib/src"
)

add_executable(${PROJECT_NAME})
target_sources(${PROJECT_NAME} PRIVATE ${LOCAL_SOURCES})
target_include_directories(${PROJECT_NAME} PRIVATE ${LOCAL_INCLUDE})
target_link_libraries(${PROJECT_NAME} raylib)

target_compile_definitions(${PROJECT_NAME} PUBLIC DATA_PATH="./data/")
add_custom_command(TARGET ${PROJECT_NAME} POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_directory
    ${CMAKE_CURRENT_LIST_DIR}/data
    $<TARGET_FILE_DIR:${PROJECT_NAME}>/data
)

if (UNIX)
    target_link_libraries(${PROJECT_NAME} m)
endif()
